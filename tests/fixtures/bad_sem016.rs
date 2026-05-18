// BAD: create_extension returns custom extensions but omits AuthorizeCall::new().

impl<LocalCall> CreateAuthorizedTransaction<LocalCall> for Test
where
    RuntimeCall: From<LocalCall>,
{
    fn create_extension() -> Self::Extension {
        (
            crate::GameAsInvited::new(None),
            indiv_pallet_score::ScoreAsParticipant::new(None),
            DenyNotFundedAccount,
        )
    }
}
