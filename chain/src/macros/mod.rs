/// A macro that conditionally inserts a random delay between two asynchronous expressions during tests.
///
/// When compiled in test mode (`#[cfg(test)]`), a random delay is inserted using the provided context's
/// `sleep` method. In non-test builds, the expressions are executed sequentially without delay.
/// The macro discards the return values of the expressions so that it always returns `()`.
///
/// # Example
///
/// ```rust
/// // Assume that `self.context` is defined and provides an async sleep method,
/// // and that `batch_network` and `app_mailbox` are in scope.
/// maybe_delay_between!(self.context,
///     app_mailbox.broadcast(batch.digest).await;
///     batch_network.0.send(Recipients::All, batch.serialize().into(), true)
///         .await.expect("failed to broadcast batch")
/// );
/// ```
///
/// You can also include an optional trailing semicolon after the second expression:
///
/// ```rust
/// maybe_delay_between!(self.context,
///     app_mailbox.broadcast(batch.digest).await;
///     batch_network.0.send(Recipients::All, batch.serialize().into(), true)
///         .await.expect("failed to broadcast batch");
/// );
/// ```
#[macro_export]
macro_rules! maybe_delay_between {
    ($context:expr, $first:expr; $second:expr $(;)?) => {{
        // Execute the first asynchronous expression and discard its value.
        let _ = $first;
        // If we're in a test build, add a random delay using the provided context.
        #[cfg(test)]
        {
            use rand::Rng;
            let delay_ms = rand::thread_rng().gen_range(50..150); // random delay between 50 and 150 ms
            $context.sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
        // Execute the second asynchronous expression and discard its value.
        let _ = $second;
        // Return unit.
        ()
    }};
}
