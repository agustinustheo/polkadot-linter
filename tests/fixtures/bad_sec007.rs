// BAD: `let _ =` silently swallows errors from Result-returning calls.
// If `?` is forgotten, the error disappears without compiler warning.

pub fn process_member(who: &T::AccountId) {
    let _ = T::Currency::reserve(who, deposit);
    let _ = T::Currency::transfer(who, &pot, amount, Preservation::Expendable);
    let _ = Members::<T>::try_mutate(who, |m| {
        m.status = Status::Active;
        Ok(())
    });
}
