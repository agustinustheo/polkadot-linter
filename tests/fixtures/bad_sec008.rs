// BAD: Panic-capable code in production pallet code.
// .unwrap(), .expect(), panic!() can crash the node or halt block production.

pub fn get_member_name(who: &T::AccountId) -> Vec<u8> {
    let member = Members::<T>::get(who).unwrap();
    let name = member.name.expect("member should have a name");
    name
}

pub fn do_critical_thing() {
    if something_wrong() {
        panic!("this should never happen");
    }
}

pub fn not_done_yet() {
    todo!("implement this before mainnet");
}
