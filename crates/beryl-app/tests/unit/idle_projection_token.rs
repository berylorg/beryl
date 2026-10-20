use super::*;

struct OwnedRegistration {
    connection: ConnectionGeneration,
    owner: SyndicThreadId,
    key: LoadedThreadKey,
    generation: CasLoadedSessionGeneration,
    token: LeaseToken,
}

impl OwnedRegistration {
    fn new() -> Self {
        let connection = allocate_connection_generation().unwrap();
        let owner = SyndicThreadId::from_bytes([250; 16]);
        let key = LoadedThreadKey {
            runtime_id: RuntimeId::from_bytes([249; 16]),
            process_generation: CasProcessGeneration::new(98_000 + connection.get()).unwrap(),
            cas_thread_id: CasThreadId::new("idle-projection-original").unwrap(),
        };
        let (generation, token) =
            register_new(key.clone(), connection, owner, Default::default()).unwrap();
        Self {
            connection,
            owner,
            key,
            generation,
            token,
        }
    }

    fn exclusive(&self) -> bool {
        owns_only_loaded_lease(
            &self.key,
            self.connection,
            self.owner,
            self.generation,
            self.token,
        )
        .unwrap()
    }
}

impl Drop for OwnedRegistration {
    fn drop(&mut self) {
        invalidate_connection(self.connection).unwrap();
    }
}

#[test]
fn idle_release_requires_the_original_token_even_when_another_lease_is_the_only_one_left() {
    let original = OwnedRegistration::new();
    assert!(original.exclusive());
    let ExistingSubscription::Exact { generation, token } =
        acquire_existing(&original.key, original.connection, original.owner).unwrap()
    else {
        panic!("original registry lease must be shareable");
    };
    assert!(!original.exclusive());
    assert!(matches!(
        release_exact(
            &original.key,
            original.connection,
            original.owner,
            original.generation,
            original.token
        )
        .unwrap(),
        ReleaseDisposition::Shared
    ));
    assert!(!original.exclusive());
    assert!(
        owns_only_loaded_lease(
            &original.key,
            original.connection,
            original.owner,
            generation,
            token
        )
        .unwrap()
    );
    assert!(matches!(
        release_exact(
            &original.key,
            original.connection,
            original.owner,
            generation,
            token
        )
        .unwrap(),
        ReleaseDisposition::Last
    ));
    let (successor, successor_token) = register_new(
        original.key.clone(),
        original.connection,
        original.owner,
        Default::default(),
    )
    .unwrap();
    assert_ne!(successor, original.generation);
    assert!(!original.exclusive());
    assert!(
        owns_only_loaded_lease(
            &original.key,
            original.connection,
            original.owner,
            successor,
            successor_token
        )
        .unwrap()
    );
}

#[test]
fn another_loaded_target_or_foreign_connection_prevents_own_idle_projection_release() {
    let original = OwnedRegistration::new();
    let mut other_key = original.key.clone();
    other_key.cas_thread_id = CasThreadId::new("idle-projection-external-target").unwrap();
    let other_owner = SyndicThreadId::from_bytes([248; 16]);
    let (generation, token) = register_new(
        other_key.clone(),
        original.connection,
        other_owner,
        Default::default(),
    )
    .unwrap();
    assert!(!original.exclusive());
    release_exact(
        &other_key,
        original.connection,
        other_owner,
        generation,
        token,
    )
    .unwrap();
    assert!(original.exclusive());
    assert!(
        !owns_only_loaded_lease(
            &original.key,
            allocate_connection_generation().unwrap(),
            original.owner,
            original.generation,
            original.token
        )
        .unwrap()
    );
    assert!(
        !owns_only_loaded_lease(
            &original.key,
            original.connection,
            other_owner,
            original.generation,
            original.token
        )
        .unwrap()
    );
}
