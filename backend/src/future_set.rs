use futures::FutureExt;
use futures::future::{Fuse, FusedFuture, join_all};
use std::pin::{Pin, pin};
use std::task::{Context, Poll};

pub struct FutureWorkers<F> {
    futures: Vec<Fuse<F>>,
}

impl<F> FutureWorkers<F> {
    pub fn new() -> Self {
        Self {
            futures: Vec::new(),
        }
    }

    pub fn add_future(&mut self, future: F)
    where
        F: Future<Output = anyhow::Result<()>> + Unpin,
    {
        self.futures.push(future.fuse());
    }
}

impl<F> Future for FutureWorkers<F>
where
    F: Future<Output = anyhow::Result<()>> + Unpin,
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.futures.is_empty() {
            // Poll all futures and ignore their output
            {
                let join = pin!(join_all(self.futures.iter_mut()));
                let _ = join.poll(cx);
            }

            // Retain only the futures that are not yet terminated
            self.futures.retain(|f| !f.is_terminated());
        }

        Poll::Pending
    }
}
