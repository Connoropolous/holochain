//! A collection of definitions related to remote communication.

/// Error related to remote communication.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TransportError {
    /// GhostError.
    #[error(transparent)]
    GhostError(#[from] ghost_actor::GhostError),

    /// Unspecified error.
    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

impl TransportError {
    /// promote a custom error type to a TransportError
    pub fn other(e: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self::Other(e.into())
    }
}

impl From<String> for TransportError {
    fn from(s: String) -> Self {
        #[derive(Debug, thiserror::Error)]
        struct OtherError(String);
        impl std::fmt::Display for OtherError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        TransportError::other(OtherError(s))
    }
}

impl From<&str> for TransportError {
    fn from(s: &str) -> Self {
        s.to_string().into()
    }
}

impl From<TransportError> for () {
    fn from(_: TransportError) {}
}

/// Result type for remote communication.
pub type TransportResult<T> = Result<T, TransportError>;

/// Defines an established connection to a remote peer.
pub mod transport_connection {
    use futures::{future::FutureExt, sink::SinkExt, stream::StreamExt};

    /// Receiver side of the channel
    pub type TransportChannelRead =
        Box<dyn futures::stream::Stream<Item = Vec<u8>> + Send + Unpin + 'static>;

    /// Extension trait for channel readers
    pub trait TransportChannelReadExt {
        /// Read the stream to close into a single byte vec.
        fn read_to_end(
            self,
        ) -> ghost_actor::dependencies::must_future::MustBoxFuture<'static, Vec<u8>>;
    }

    impl<T: futures::stream::Stream<Item = Vec<u8>> + Send + Unpin + 'static>
        TransportChannelReadExt for T
    {
        fn read_to_end(
            self,
        ) -> ghost_actor::dependencies::must_future::MustBoxFuture<'static, Vec<u8>> {
            async move {
                self.fold(Vec::new(), |mut acc, x| async move {
                    acc.extend_from_slice(&x);
                    acc
                })
                .await
            }
            .boxed()
            .into()
        }
    }

    /// Sender side of the channel
    pub type TransportChannelWrite = Box<
        dyn futures::sink::Sink<Vec<u8>, Error = super::TransportError> + Send + Unpin + 'static,
    >;

    /// Extension trait for channel writers
    pub trait TransportChannelWriteExt {
        /// Write all data and close channel
        fn write_and_close<'a>(
            &'a mut self,
            data: Vec<u8>,
        ) -> ghost_actor::dependencies::must_future::MustBoxFuture<'a, super::TransportResult<()>>;
    }

    impl<
            T: futures::sink::Sink<Vec<u8>, Error = super::TransportError> + Send + Unpin + 'static,
        > TransportChannelWriteExt for T
    {
        fn write_and_close<'a>(
            &'a mut self,
            data: Vec<u8>,
        ) -> ghost_actor::dependencies::must_future::MustBoxFuture<'a, super::TransportResult<()>>
        {
            async move {
                self.send(data).await?;
                self.close().await?;
                Ok(())
            }
            .boxed()
            .into()
        }
    }

    ghost_actor::ghost_chan! {
        /// Event stream for handling incoming requests from a remote.
        pub chan TransportConnectionEvent<super::TransportError> {
            /// Event for receiving an incoming transport channel.
            fn incoming_channel(
                url: url2::Url2,
                send: TransportChannelWrite,
                recv: TransportChannelRead,
            ) -> ();
        }
    }

    /// Receiver type for incoming connection events.
    pub type TransportConnectionEventReceiver =
        futures::channel::mpsc::Receiver<TransportConnectionEvent>;

    ghost_actor::ghost_chan! {
        /// Represents a connection to a remote node.
        pub chan TransportConnection<super::TransportError> {
            /// Retrieve the current url (address) of the remote end of this connection.
            fn remote_url() -> url2::Url2;

            /// Create a new outgoing transport channel on this connection.
            fn create_channel() -> (
                TransportChannelWrite,
                TransportChannelRead,
            );
        }
    }

    /// Extension trait for additional methods on TransportConnections
    pub trait TransportConnectionSenderExt {
        /// Make a request using a single channel open/close
        fn request(
            &self,
            data: Vec<u8>,
        ) -> ghost_actor::dependencies::must_future::MustBoxFuture<
            'static,
            super::TransportResult<Vec<u8>>,
        >;
    }

    impl<T: TransportConnectionSender> TransportConnectionSenderExt for T {
        fn request(
            &self,
            data: Vec<u8>,
        ) -> ghost_actor::dependencies::must_future::MustBoxFuture<
            'static,
            super::TransportResult<Vec<u8>>,
        > {
            use super::TransportError;

            let fut = self.create_channel();
            async move {
                let (mut send, recv) = fut.await.map_err(TransportError::other)?;
                send.write_and_close(data).await?;
                let out = recv.read_to_end().await;

                Ok(out)
            }
            .boxed()
            .into()
        }
    }
}

/// Defines a local binding
/// (1) for accepting incoming connections and
/// (2) for making outgoing connections.
pub mod transport_listener {
    ghost_actor::ghost_chan! {
        /// Event stream for handling incoming connections.
        pub chan TransportListenerEvent<super::TransportError> {
            /// Event for handling incoming connections from a remote.
            fn incoming_connection(
                sender: ghost_actor::GhostSender<super::transport_connection::TransportConnection>,
                receiver: super::transport_connection::TransportConnectionEventReceiver,
            ) -> ();
        }
    }

    /// Receiver type for incoming listener events.
    pub type TransportListenerEventReceiver =
        futures::channel::mpsc::Receiver<TransportListenerEvent>;

    ghost_actor::ghost_chan! {
        /// Represents a socket binding for establishing connections.
        pub chan TransportListener<super::TransportError> {
            /// Retrieve the current url (address) this listener is bound to.
            fn bound_url() -> url2::Url2;

            /// Attempt to establish an outgoing connection to a remote.
            fn connect(url: url2::Url2) -> (
                ghost_actor::GhostSender<super::transport_connection::TransportConnection>,
                super::transport_connection::TransportConnectionEventReceiver,
            );
        }
    }
}
