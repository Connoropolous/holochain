use hdk3::prelude::*;

#[hdk_entry(id = "post", required_validations = 5)]
struct Post(String);

#[hdk_entry(id = "msg", required_validations = 5)]
struct Msg(String);

entry_defs![Post::entry_def(), Msg::entry_def()];

fn post() -> Post {
    Post("foo".into())
}

#[hdk_extern]
fn create_entry(_: ()) -> ExternResult<HeaderHash> {
    Ok(create_entry!(post())?)
}

#[hdk_extern]
fn get_entry(_: ()) -> ExternResult<GetOutput> {
    Ok(GetOutput::new(get!(hash_entry!(post())?)?))
}
