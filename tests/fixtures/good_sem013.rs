// GOOD: The custom invalidity enum uses an explicit u8 representation.

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum CustomInvalidity {
    InvalidAlias = 1,
    StaleProof = 2,
}

impl From<CustomInvalidity> for TransactionValidityError {
    fn from(value: CustomInvalidity) -> Self {
        InvalidTransaction::Custom(value as u8).into()
    }
}
