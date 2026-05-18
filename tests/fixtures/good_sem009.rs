// GOOD: Calls remove() directly without redundant contains_key().
// remove() is idempotent — it does nothing if the key doesn't exist.

pub fn clean_up(key: &T::AccountId) {
    Members::<T>::remove(key);
}

pub fn deregister(who: &T::AccountId) {
    if let Some(registration) = Registrations::<T>::take(who) {
        Self::deposit_event(Event::Deregistered { who: who.clone() });
    }
}
