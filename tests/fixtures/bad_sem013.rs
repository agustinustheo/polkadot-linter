// BAD: The custom invalidity enum is converted into InvalidTransaction::Custom
// but the discriminant layout is implicit because #[repr(u8)] is missing.

#[derive(Clone, Copy)]
pub enum CustomInvalidity {
    InvalidAlias = 1,
    StaleProof = 2,
}

impl From<CustomInvalidity> for TransactionValidityError {
    fn from(value: CustomInvalidity) -> Self {
        InvalidTransaction::Custom(value as u8).into()
    }
}
