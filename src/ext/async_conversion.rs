/// Async counterpart to the standard library's `From<T>` trait.
///
/// This trait allows for asynchronous conversion from one type to another.
/// It's useful when the conversion process involves I/O operations, network calls,
/// or other async operations.
///
/// # Examples
///
/// ```rust
/// use std::path::PathBuf;
///
/// struct FileContent(String);
///
/// impl AsyncFrom<PathBuf> for FileContent {
///     type Error = std::io::Error;
///     
///     async fn async_from(path: PathBuf) -> Result<Self, Self::Error> {
///         let content = tokio::fs::read_to_string(path).await?;
///         Ok(FileContent(content))
///     }
/// }
/// ```
pub trait AsyncFrom<T>: Sized {
    /// The error type that can occur during conversion.
    type Error;

    /// Performs the asynchronous conversion from `T` to `Self`.
    async fn async_from(value: T) -> Result<Self, Self::Error>;
}

/// Async counterpart to the standard library's `Into<T>` trait.
///
/// This trait allows for asynchronous conversion from `Self` to another type.
/// Like the standard `Into` trait, this is typically implemented automatically
/// when `AsyncFrom` is implemented for the target type.
///
/// # Examples
///
/// ```rust
/// use std::path::PathBuf;
///
/// struct DatabaseRecord {
///     id: u32,
///     data: String,
/// }
///
/// impl AsyncInto<PathBuf> for DatabaseRecord {
///     type Error = std::io::Error;
///     
///     async fn async_into(self) -> Result<PathBuf, Self::Error> {
///         let filename = format!("record_{}.json", self.id);
///         let path = PathBuf::from(filename);
///         tokio::fs::write(&path, self.data).await?;
///         Ok(path)
///     }
/// }
/// ```
pub trait AsyncInto<T> {
    /// The error type that can occur during conversion.
    type Error;

    /// Performs the asynchronous conversion from `Self` to `T`.
    async fn async_into(self) -> Result<T, Self::Error>;
}

/// Blanket implementation that provides `AsyncInto<U>` for any type `T`
/// where `U` implements `AsyncFrom<T>`.
///
/// This mirrors the standard library's blanket implementation for `Into<T>`.
impl<T, U> AsyncInto<U> for T
where
    U: AsyncFrom<T>,
{
    type Error = U::Error;

    async fn async_into(self) -> Result<U, Self::Error> {
        U::async_from(self).await
    }
}

/// A convenience trait that combines both `AsyncFrom` and `AsyncInto` functionality.
///
/// This trait is useful when you want to express that a type can be converted
/// both ways asynchronously.
pub trait AsyncTryFrom<T>: Sized {
    /// The error type that can occur during conversion.
    type Error;

    /// Performs the fallible asynchronous conversion from `T` to `Self`.
    async fn async_try_from(value: T) -> Result<Self, Self::Error>;
}

/// Async counterpart to `TryInto<T>`.
pub trait AsyncTryInto<T> {
    /// The error type that can occur during conversion.
    type Error;

    /// Performs the fallible asynchronous conversion from `Self` to `T`.
    async fn async_try_into(self) -> Result<T, Self::Error>;
}

/// Blanket implementation for `AsyncTryInto<U>` when `U` implements `AsyncTryFrom<T>`.
impl<T, U> AsyncTryInto<U> for T
where
    U: AsyncTryFrom<T>,
{
    type Error = U::Error;

    async fn async_try_into(self) -> Result<U, Self::Error> {
        U::async_try_from(self).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Example types for testing
    struct StringWrapper(String);
    struct NumberWrapper(i32);

    impl AsyncFrom<String> for StringWrapper {
        type Error = ();

        fn async_from(value: String) -> impl Future<Output = Result<Self, Self::Error>> + Send {
            async move { Ok(StringWrapper(value)) }
        }
    }

    impl AsyncTryFrom<String> for NumberWrapper {
        type Error = std::num::ParseIntError;

        fn async_try_from(value: String) -> impl Future<Output = Result<Self, Self::Error>> + Send {
            async move {
                let number = value.parse::<i32>()?;
                Ok(NumberWrapper(number))
            }
        }
    }

    #[test]
    fn test_async_from() {
        // Create a simple executor for testing
        futures::executor::block_on(async {
            let input = "hello".to_string();
            let wrapper = StringWrapper::async_from(input).await.unwrap();
            assert_eq!(wrapper.0, "hello");
        });
    }

    #[test]
    fn test_async_into() {
        futures::executor::block_on(async {
            let input = "world".to_string();
            let wrapper: Result<StringWrapper, ()> = input.async_into().await;
            assert_eq!(wrapper.unwrap().0, "world");
        });
    }

    #[test]
    fn test_async_try_from_success() {
        futures::executor::block_on(async {
            let input = "42".to_string();
            let wrapper = NumberWrapper::async_try_from(input).await.unwrap();
            assert_eq!(wrapper.0, 42);
        });
    }

    #[test]
    fn test_async_try_from_failure() {
        futures::executor::block_on(async {
            let input = "not_a_number".to_string();
            let result = NumberWrapper::async_try_from(input).await;
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_async_try_into() {
        futures::executor::block_on(async {
            let input = "123".to_string();
            let wrapper: Result<NumberWrapper, _> = input.async_try_into().await;
            assert_eq!(wrapper.unwrap().0, 123);
        });
    }
}
