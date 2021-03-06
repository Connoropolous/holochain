use ::fixt::prelude::*;
use hdk3::prelude::*;
use holo_hash::fixt::*;
use holochain::conductor::{
    api::{AppInterfaceApi, AppRequest, AppResponse, RealAppInterfaceApi},
    dna_store::MockDnaStore,
    ConductorBuilder, ConductorHandle,
};
use holochain::core::ribosome::ZomeCallInvocation;
use holochain::fixt::*;
use holochain_state::test_utils::{test_conductor_env, test_wasm_env, TestEnvironment};
use holochain_types::app::InstalledCell;
use holochain_types::cell::CellId;
use holochain_types::dna::DnaDef;
use holochain_types::dna::DnaFile;
use holochain_types::test_utils::fake_agent_pubkey_1;
use holochain_types::{observability, test_utils::fake_agent_pubkey_2};
use holochain_wasm_test_utils::TestWasm;
pub use holochain_zome_types::capability::CapSecret;
use holochain_zome_types::ExternInput;
use holochain_zome_types::ZomeCallResponse;
use std::sync::Arc;
use tempdir::TempDir;

#[derive(Serialize, Deserialize, SerializedBytes)]
struct CreateMessageInput {
    channel_hash: EntryHash,
    content: String,
}

#[derive(Debug, Serialize, Deserialize, SerializedBytes)]
pub struct ChannelName(String);

#[tokio::test(threaded_scheduler)]
async fn ser_entry_hash_test() {
    observability::test_run().ok();
    let eh = fixt!(EntryHash);
    let sb: SerializedBytes = eh.clone().try_into().unwrap();
    tracing::debug!(?sb);
    let o: HashEntryOutput = sb.try_into().unwrap();
    let sb: SerializedBytes = o.try_into().unwrap();
    tracing::debug!(?sb);
    let _eh: EntryHash = sb.try_into().unwrap();
}

#[tokio::test(threaded_scheduler)]
/// we can call a fn on a remote
async fn ser_regression_test() {
    observability::test_run().ok();
    // ////////////
    // START DNA
    // ////////////

    let dna_file = DnaFile::new(
        DnaDef {
            name: "ser_regression_test".to_string(),
            uuid: "ba1d046d-ce29-4778-914b-47e6010d2faf".to_string(),
            properties: SerializedBytes::try_from(()).unwrap(),
            zomes: vec![TestWasm::SerRegression.into()].into(),
        },
        vec![TestWasm::SerRegression.into()],
    )
    .await
    .unwrap();

    // //////////
    // END DNA
    // //////////

    // ///////////
    // START ALICE
    // ///////////

    let alice_agent_id = fake_agent_pubkey_1();
    let alice_cell_id = CellId::new(dna_file.dna_hash().to_owned(), alice_agent_id.clone());
    let alice_installed_cell = InstalledCell::new(alice_cell_id.clone(), "alice_handle".into());

    // /////////
    // END ALICE
    // /////////

    // /////////
    // START BOB
    // /////////

    let bob_agent_id = fake_agent_pubkey_2();
    let bob_cell_id = CellId::new(dna_file.dna_hash().to_owned(), bob_agent_id.clone());
    let bob_installed_cell = InstalledCell::new(bob_cell_id.clone(), "bob_handle".into());

    // ///////
    // END BOB
    // ///////

    // ///////////////
    // START CONDUCTOR
    // ///////////////

    let mut dna_store = MockDnaStore::new();

    dna_store.expect_get().return_const(Some(dna_file.clone()));
    dna_store
        .expect_add_dnas::<Vec<_>>()
        .times(2)
        .return_const(());
    dna_store
        .expect_add_entry_defs::<Vec<_>>()
        .times(2)
        .return_const(());
    dna_store.expect_get_entry_def().return_const(None);

    let (_tmpdir, app_api, handle) = setup_app(
        vec![(alice_installed_cell, None), (bob_installed_cell, None)],
        dna_store,
    )
    .await;

    // /////////////
    // END CONDUCTOR
    // /////////////

    // ALICE DOING A CALL

    let channel = ChannelName("hello world".into());

    let invocation = ZomeCallInvocation {
        cell_id: alice_cell_id.clone(),
        zome_name: TestWasm::SerRegression.into(),
        cap: Some(CapSecretFixturator::new(Unpredictable).next().unwrap()),
        fn_name: "create_channel".into(),
        payload: ExternInput::new(channel.try_into().unwrap()),
        provenance: alice_agent_id.clone(),
    };

    let request = Box::new(invocation.clone());
    let request = AppRequest::ZomeCallInvocation(request).try_into().unwrap();
    let response = app_api.handle_app_request(request).await;

    let _channel_hash = match response {
        AppResponse::ZomeCallInvocation(r) => {
            let response: SerializedBytes = r.into_inner();
            let channel_hash: EntryHash = response.try_into().unwrap();
            channel_hash
        }
        _ => unreachable!(),
    };

    let output = handle.call_zome(invocation).await.unwrap().unwrap();

    let channel_hash = match output {
        ZomeCallResponse::Ok(guest_output) => {
            let response: SerializedBytes = guest_output.into_inner();
            let channel_hash: EntryHash = response.try_into().unwrap();
            channel_hash
        }
        _ => unreachable!(),
    };

    let message = CreateMessageInput {
        channel_hash,
        content: "Hello from alice :)".into(),
    };
    let invocation = ZomeCallInvocation {
        cell_id: alice_cell_id.clone(),
        zome_name: TestWasm::SerRegression.into(),
        cap: Some(CapSecretFixturator::new(Unpredictable).next().unwrap()),
        fn_name: "create_message".into(),
        payload: ExternInput::new(message.try_into().unwrap()),
        provenance: alice_agent_id.clone(),
    };

    let request = Box::new(invocation.clone());
    let request = AppRequest::ZomeCallInvocation(request).try_into().unwrap();
    let response = app_api.handle_app_request(request).await;

    let _msg_hash = match response {
        AppResponse::ZomeCallInvocation(r) => {
            let response: SerializedBytes = r.into_inner();
            let msg_hash: EntryHash = response.try_into().unwrap();
            msg_hash
        }
        _ => unreachable!(),
    };

    let output = handle.call_zome(invocation).await.unwrap().unwrap();

    match output {
        ZomeCallResponse::Ok(guest_output) => {
            let response: SerializedBytes = guest_output.into_inner();
            let _msg_hash: EntryHash = response.try_into().unwrap();
        }
        _ => unreachable!(),
    };

    let shutdown = handle.take_shutdown_handle().await.unwrap();
    handle.shutdown().await;
    shutdown.await.unwrap();
}

pub async fn setup_app(
    cell_data: Vec<(InstalledCell, Option<SerializedBytes>)>,
    dna_store: MockDnaStore,
) -> (Arc<TempDir>, RealAppInterfaceApi, ConductorHandle) {
    let test_env = test_conductor_env();
    let TestEnvironment {
        env: wasm_env,
        tmpdir: _tmpdir,
    } = test_wasm_env();
    let tmpdir = test_env.tmpdir.clone();

    let conductor_handle = ConductorBuilder::with_mock_dna_store(dna_store)
        .test(test_env, wasm_env)
        .await
        .unwrap();

    conductor_handle
        .clone()
        .install_app("test app".to_string(), cell_data)
        .await
        .unwrap();

    conductor_handle
        .activate_app("test app".to_string())
        .await
        .unwrap();

    let errors = conductor_handle.clone().setup_cells().await.unwrap();

    assert!(errors.is_empty());

    let handle = conductor_handle.clone();

    (
        tmpdir,
        RealAppInterfaceApi::new(conductor_handle, "test-interface".into()),
        handle,
    )
}
