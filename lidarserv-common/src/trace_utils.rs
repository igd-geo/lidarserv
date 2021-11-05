pub use tracy_client;

/// Concatenate the current module path and the given string literal.
/// Utility macro for [crate::span].
#[macro_export]
macro_rules! fqn {
    ($l: literal) => {
        ::std::concat!(::std::module_path!(), " ", $l)
    };
}

/// Creates a new trace span ([tracy_client::Span]), with the correct source file information.
#[macro_export]
macro_rules! span {
    () => {
        $crate::trace_utils::tracy_client::Span::new("", "", ::std::file!(), ::std::line!(), 0)
    };
    ($fn: literal) => {
        $crate::trace_utils::tracy_client::Span::new(
            "",
            $crate::fqn!($fn),
            ::std::file!(),
            ::std::line!(),
            16,
        )
    };
}
