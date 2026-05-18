// BAD: Calls contains_key() then remove() on the same storage item.
// The contains_key is a wasted storage read since remove is idempotent.

pub fn clean_up(key: &T::AccountId) {
    if Members::<T>::contains_key(key) {
        Members::<T>::remove(key);
    }
}

pub fn deregister(who: &T::AccountId) {
    if Registrations::<T>::contains_key(who) {
        Registrations::<T>::take(who);
        Self::deposit_event(Event::Deregistered { who: who.clone() });
    }
}
