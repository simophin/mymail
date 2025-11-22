use tokio::task::{AbortHandle, JoinHandle};

pub struct AutoAbortHandle(AbortHandle);

impl Drop for AutoAbortHandle {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub trait AbortHandleExt {
    fn auto_abort(self) -> AutoAbortHandle;
}

impl AbortHandleExt for AbortHandle {
    fn auto_abort(self) -> AutoAbortHandle {
        AutoAbortHandle(self)
    }
}

impl<T> AbortHandleExt for JoinHandle<T> {
    fn auto_abort(self) -> AutoAbortHandle {
        AutoAbortHandle(self.abort_handle())
    }
}
