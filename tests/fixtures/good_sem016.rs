// GOOD: create_extension includes AuthorizeCall::new() even in a larger tuple.

impl<LocalCall> frame_system::offchain::CreateAuthorizedTransaction<LocalCall> for Runtime
where
    RuntimeCall: From<LocalCall>,
{
    fn create_extension() -> Self::Extension {
        (
            (
                crate::SomeExtension::<Runtime>::new(None),
                frame_system::AuthorizeCall::<Runtime>::new(),
            ),
            crate::OuterExtension::<Runtime>::new(false),
        )
    }
}
