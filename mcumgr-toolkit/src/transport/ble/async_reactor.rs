pub(super) struct AsyncReactor(Box<tokio::runtime::Runtime>);

impl AsyncReactor {
    pub(super) fn new() -> Self {
        Self(Box::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap(),
        ))
    }

    pub(super) fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        self.0.block_on(future)
    }
}
