use super::*;
use aion_types::{Address, H256, U256};
use key::Ed25519Secret;
use logger::init_log;
use receipt::SimpleReceipt;
use rustc_hex::FromHex;
use std::str::FromStr;
use tests::helpers::*;
use transaction::*;

fn secret() -> Ed25519Secret {
    Ed25519Secret::from_str("7ea8af7d0982509cd815096d35bc3a295f57b2a078e4e25731e3ea977b9544626702b86f33072a55f46003b1e3e242eb18556be54c5ab12044c3c20829e0abb5").unwrap()
}

//    fn make_frontier_machine() -> Machine {
//        let machine = ::ethereum::new_frontier_test_machine();
//        machine
//    }

//    #[test]
//    fn should_apply_create_transaction() {
//        init_log();
//
//        let mut state = get_temp_state();
//        let mut info = EnvInfo::default();
//        info.gas_limit = 1_000_000.into();
//        let machine = make_frontier_machine();
//
//        let t = Transaction {
//            nonce: 0.into(),
//            nonce_bytes: Vec::new(),
//            gas_price: 0.into(),
//            gas_price_bytes: Vec::new(),
//            gas: 500_000.into(),
//            gas_bytes: Vec::new(),
//            action: Action::Create,
//            value: 100.into(),
//            value_bytes: Vec::new(),
//            transaction_type: 1.into(),
//            data: FromHex::from_hex("601080600c6000396000f3006000355415600957005b60203560003555")
//                .unwrap(),
//        }
//        .sign(&secret(), None);
//
//        state
//            .add_balance(&t.sender(), &(100.into()), CleanupMode::NoEmpty)
//            .unwrap();
//        let result = state.apply(&info, &machine, &t).unwrap();
//
//        let expected_receipt = Receipt {
//            simple_receipt: SimpleReceipt{log_bloom: "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000".into(),
//            logs: vec![], state_root: H256::from(
//                    "0xadfb0633de8b1effff5c6b4f347b435f99e48339164160ee04bac13115c90dc9"
//                ), },
//            output: vec![96, 0, 53, 84, 21, 96, 9, 87, 0, 91, 96, 32, 53, 96, 0, 53],
//            gas_used: U256::from(222506),
//            error_message:  String::new(),
//            transaction_fee: U256::from(0),
//        };
//
//        assert_eq!(result.receipt, expected_receipt);
//    }

#[test]
fn should_work_when_cloned() {
    init_log();

    let a = Address::zero();

    let mut state = {
        let mut state = get_temp_state();
        assert_eq!(state.exists(&a).unwrap(), false);
        state.inc_nonce(&a).unwrap();
        state.commit().unwrap();
        state.clone()
    };

    state.inc_nonce(&a).unwrap();
    state.commit().unwrap();
}

#[test]
fn balance_from_database() {
    let a = Address::zero();
    let (root, db) = {
        let mut state = get_temp_state();
        state
            .require_or_from(
                &a,
                false,
                || AionVMAccount::new_contract(42.into(), 0.into()),
                |_| {},
            )
            .unwrap();
        state.commit().unwrap();
        assert_eq!(state.balance(&a).unwrap(), 42.into());
        state.drop()
    };

    let state = State::from_existing(
        db,
        root,
        U256::from(0u8),
        Default::default(),
        Arc::new(MemoryDBRepository::new()),
    )
    .unwrap();
    assert_eq!(state.balance(&a).unwrap(), 42.into());
}

#[test]
fn avm_empty_bytes_or_null() {
    let a = Address::zero();
    let mut state = get_temp_state();
    state
        .require_or_from(
            &a,
            false,
            || {
                let mut acc = AionVMAccount::new_contract(42.into(), 0.into());
                acc.account_type = AccType::AVM;
                acc
            },
            |_| {},
        )
        .unwrap();
    let key = vec![0x01];
    let value = vec![];
    state.set_storage(&a, key.clone(), value).unwrap();
    assert_eq!(state.storage_at(&a, &key).unwrap(), Some(vec![]));
    state.commit().unwrap();
    state.remove_storage(&a, key.clone()).unwrap();
    // remove unexisting key
    state.remove_storage(&a, vec![0x02]).unwrap();
    state.commit().unwrap();
    state.set_storage(&a, vec![0x02], vec![0x03]).unwrap();
    // clean local cache
    state.commit().unwrap();
    assert_eq!(state.storage_at(&a, &key).unwrap(), None);
    assert_eq!(state.storage_at(&a, &vec![0x02]).unwrap(), Some(vec![0x03]));
}

#[test]
fn code_from_database() {
    let a = Address::zero();

    let (root, db) = {
        let mut state = get_temp_state();
        state
            .require_or_from(
                &a,
                false,
                || AionVMAccount::new_contract(42.into(), 0.into()),
                |_| {},
            )
            .unwrap();
        state.init_code(&a, vec![1, 2, 3]).unwrap();
        assert_eq!(state.code(&a).unwrap(), Some(Arc::new(vec![1u8, 2, 3])));
        state.commit().unwrap();
        assert_eq!(state.code(&a).unwrap(), Some(Arc::new(vec![1u8, 2, 3])));
        state.drop()
    };

    let state = State::from_existing(
        db,
        root,
        U256::from(0u8),
        Default::default(),
        Arc::new(MemoryDBRepository::new()),
    )
    .unwrap();
    assert_eq!(state.code(&a).unwrap(), Some(Arc::new(vec![1u8, 2, 3])));
}

#[test]
fn transformed_code_from_database() {
    let a = Address::zero();
    let (root, db) = {
        let mut state = get_temp_state();
        state
            .require_or_from(
                &a,
                false,
                || AionVMAccount::new_contract(42.into(), 0.into()),
                |_| {},
            )
            .unwrap();
        state.init_transformed_code(&a, vec![1, 2, 3]).unwrap();
        assert_eq!(
            state.transformed_code(&a).unwrap(),
            Some(Arc::new(vec![1u8, 2, 3]))
        );
        state.commit().unwrap();
        assert_eq!(
            state.transformed_code(&a).unwrap(),
            Some(Arc::new(vec![1u8, 2, 3]))
        );
        state.drop()
    };

    let state = State::from_existing(
        db,
        root,
        U256::from(0u8),
        Default::default(),
        Arc::new(MemoryDBRepository::new()),
    )
    .unwrap();
    assert_eq!(
        state.transformed_code(&a).unwrap(),
        Some(Arc::new(vec![1u8, 2, 3]))
    );
}

#[test]
fn storage_at_from_database() {
    let a = Address::zero();
    let (root, db) = {
        let mut state = get_temp_state_with_nonce();
        state.set_storage(&a, vec![2], vec![69]).unwrap();
        state.commit().unwrap();
        state.drop()
    };

    let s = State::from_existing(
        db,
        root,
        U256::from(0u8),
        Default::default(),
        Arc::new(MemoryDBRepository::new()),
    )
    .unwrap();
    assert_eq!(s.storage_at(&a, &vec![2]).unwrap_or(None), Some(vec![69]));
}

#[test]
fn get_from_database() {
    let a = Address::zero();
    let (root, db) = {
        let mut state = get_temp_state();
        state.inc_nonce(&a).unwrap();
        state
            .add_balance(&a, &U256::from(69u64), CleanupMode::NoEmpty)
            .unwrap();
        state.commit().unwrap();
        assert_eq!(state.balance(&a).unwrap(), U256::from(69u64));
        state.drop()
    };

    let state = State::from_existing(
        db,
        root,
        U256::from(1u8),
        Default::default(),
        Arc::new(MemoryDBRepository::new()),
    )
    .unwrap();
    assert_eq!(state.balance(&a).unwrap(), U256::from(69u64));
    assert_eq!(state.nonce(&a).unwrap(), U256::from(1u64));
}

#[test]
fn remove() {
    let a = Address::zero();
    let mut state = get_temp_state();
    assert_eq!(state.exists(&a).unwrap(), false);
    assert_eq!(state.exists_and_not_null(&a).unwrap(), false);
    state.inc_nonce(&a).unwrap();
    assert_eq!(state.exists(&a).unwrap(), true);
    assert_eq!(state.exists_and_not_null(&a).unwrap(), true);
    assert_eq!(state.nonce(&a).unwrap(), U256::from(1u64));
    state.kill_account(&a);
    assert_eq!(state.exists(&a).unwrap(), false);
    assert_eq!(state.exists_and_not_null(&a).unwrap(), false);
    assert_eq!(state.nonce(&a).unwrap(), U256::from(0u64));
}

#[test]
fn empty_account_is_not_created() {
    let a = Address::zero();
    let db = get_temp_state_db();
    let (root, db) = {
        let mut state = State::new(
            db,
            U256::from(0),
            Default::default(),
            Arc::new(MemoryDBRepository::new()),
        );
        state
            .add_balance(&a, &U256::default(), CleanupMode::NoEmpty)
            .unwrap(); // create an empty account
        state.commit().unwrap();
        state.drop()
    };
    let state = State::from_existing(
        db,
        root,
        U256::from(0u8),
        Default::default(),
        Arc::new(MemoryDBRepository::new()),
    )
    .unwrap();
    assert!(!state.exists(&a).unwrap());
    assert!(!state.exists_and_not_null(&a).unwrap());
}

#[test]
fn empty_account_exists_when_creation_forced() {
    let a = Address::zero();
    let db = get_temp_state_db();
    let (root, db) = {
        println!("default balance = {}", U256::default());
        let mut state = State::new(
            db,
            U256::from(0),
            Default::default(),
            Arc::new(MemoryDBRepository::new()),
        );
        state
            .add_balance(&a, &U256::default(), CleanupMode::ForceCreate)
            .unwrap(); // create an empty account
        state.commit().unwrap();
        state.drop()
    };
    let state = State::from_existing(
        db,
        root,
        U256::from(0u8),
        Default::default(),
        Arc::new(MemoryDBRepository::new()),
    )
    .unwrap();

    assert!(!state.exists(&a).unwrap());
    assert!(!state.exists_and_not_null(&a).unwrap());
}

#[test]
fn remove_from_database() {
    let a = Address::zero();
    let (root, db) = {
        let mut state = get_temp_state();
        state.inc_nonce(&a).unwrap();
        state.commit().unwrap();
        assert_eq!(state.exists(&a).unwrap(), true);
        assert_eq!(state.nonce(&a).unwrap(), U256::from(1u64));
        state.drop()
    };

    let (root, db) = {
        let mut state = State::from_existing(
            db,
            root,
            U256::from(0u8),
            Default::default(),
            Arc::new(MemoryDBRepository::new()),
        )
        .unwrap();
        assert_eq!(state.exists(&a).unwrap(), true);
        assert_eq!(state.nonce(&a).unwrap(), U256::from(1u64));
        state.kill_account(&a);
        state.commit().unwrap();
        assert_eq!(state.exists(&a).unwrap(), false);
        assert_eq!(state.nonce(&a).unwrap(), U256::from(0u64));
        state.drop()
    };

    let state = State::from_existing(
        db,
        root,
        U256::from(0u8),
        Default::default(),
        Arc::new(MemoryDBRepository::new()),
    )
    .unwrap();
    assert_eq!(state.exists(&a).unwrap(), false);
    assert_eq!(state.nonce(&a).unwrap(), U256::from(0u64));
}

#[test]
fn alter_balance() {
    let mut state = get_temp_state();
    let a = Address::zero();
    let b = 1u64.into();
    state
        .add_balance(&a, &U256::from(69u64), CleanupMode::NoEmpty)
        .unwrap();
    assert_eq!(state.balance(&a).unwrap(), U256::from(69u64));
    state.commit().unwrap();
    assert_eq!(state.balance(&a).unwrap(), U256::from(69u64));
    state
        .sub_balance(&a, &U256::from(42u64), &mut CleanupMode::NoEmpty)
        .unwrap();
    assert_eq!(state.balance(&a).unwrap(), U256::from(27u64));
    state.commit().unwrap();
    assert_eq!(state.balance(&a).unwrap(), U256::from(27u64));
    state
        .transfer_balance(&a, &b, &U256::from(18u64), CleanupMode::NoEmpty)
        .unwrap();
    assert_eq!(state.balance(&a).unwrap(), U256::from(9u64));
    assert_eq!(state.balance(&b).unwrap(), U256::from(18u64));
    state.commit().unwrap();
    assert_eq!(state.balance(&a).unwrap(), U256::from(9u64));
    assert_eq!(state.balance(&b).unwrap(), U256::from(18u64));
}

#[test]
fn alter_nonce() {
    let mut state = get_temp_state();
    let a = Address::zero();
    state.inc_nonce(&a).unwrap();
    assert_eq!(state.nonce(&a).unwrap(), U256::from(1u64));
    state.inc_nonce(&a).unwrap();
    assert_eq!(state.nonce(&a).unwrap(), U256::from(2u64));
    state.commit().unwrap();
    assert_eq!(state.nonce(&a).unwrap(), U256::from(2u64));
    state.inc_nonce(&a).unwrap();
    assert_eq!(state.nonce(&a).unwrap(), U256::from(3u64));
    state.commit().unwrap();
    assert_eq!(state.nonce(&a).unwrap(), U256::from(3u64));
}

#[test]
fn balance_nonce() {
    let mut state = get_temp_state();
    let a = Address::zero();
    assert_eq!(state.balance(&a).unwrap(), U256::from(0u64));
    assert_eq!(state.nonce(&a).unwrap(), U256::from(0u64));
    state.commit().unwrap();
    assert_eq!(state.balance(&a).unwrap(), U256::from(0u64));
    assert_eq!(state.nonce(&a).unwrap(), U256::from(0u64));
}

#[test]
fn ensure_cached() {
    let mut state = get_temp_state_with_nonce();
    let a = Address::zero();
    state.require(&a, false).unwrap();
    state.commit().unwrap();
    assert_eq!(
        *state.root(),
        "9d6d4b335038e1ffe0f060c29e52d6eed2aec4a085dfa37afba9d1e10cc7be85".into()
    );
}

#[test]
fn checkpoint_basic() {
    let mut state = get_temp_state();
    let a = Address::zero();
    state.checkpoint();
    state
        .add_balance(&a, &U256::from(69u64), CleanupMode::NoEmpty)
        .unwrap();
    assert_eq!(state.balance(&a).unwrap(), U256::from(69u64));
    state.discard_checkpoint();
    assert_eq!(state.balance(&a).unwrap(), U256::from(69u64));
    state.checkpoint();
    state
        .add_balance(&a, &U256::from(1u64), CleanupMode::NoEmpty)
        .unwrap();
    assert_eq!(state.balance(&a).unwrap(), U256::from(70u64));
    state.revert_to_checkpoint();
    assert_eq!(state.balance(&a).unwrap(), U256::from(69u64));
}

#[test]
fn checkpoint_nested() {
    let mut state = get_temp_state();
    let a = Address::zero();
    state.checkpoint();
    state.checkpoint();
    state
        .add_balance(&a, &U256::from(69u64), CleanupMode::NoEmpty)
        .unwrap();
    assert_eq!(state.balance(&a).unwrap(), U256::from(69u64));
    state.discard_checkpoint();
    assert_eq!(state.balance(&a).unwrap(), U256::from(69u64));
    state.revert_to_checkpoint();
    assert_eq!(state.balance(&a).unwrap(), U256::from(0));
}

#[test]
fn create_empty() {
    let mut state = get_temp_state();
    state.commit().unwrap();
    assert_eq!(
        *state.root(),
        "45b0cfc220ceec5b7c1c62c4d4193d38e4eba48e8815729ce75f9c0ab0e4c1c0".into()
    );
}

#[test]
fn should_not_panic_on_state_diff_with_storage() {
    let mut state = get_temp_state();

    let a: Address = 0xa.into();
    state.init_code(&a, b"abcdefg".to_vec()).unwrap();;
    state
        .add_balance(&a, &256.into(), CleanupMode::NoEmpty)
        .unwrap();
    state.set_storage(&a, vec![0x0b], vec![0x0c]).unwrap();

    let mut new_state = state.clone();
    new_state.set_storage(&a, vec![0x0b], vec![0x0d]).unwrap();

    new_state.diff_from(state).unwrap();
}