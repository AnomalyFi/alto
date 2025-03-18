pub mod actor;
pub mod ingress;
pub mod coordinator;
pub mod collector;

#[cfg(test)]
mod tests {
    use std::{collections::{BTreeMap, HashMap}, hash::Hash, sync::{Arc, Mutex}, time::Duration};
    use bytes::Bytes;
    use commonware_broadcast::linked::{Config, Engine};
    use tracing::debug;

    use commonware_cryptography::{bls12381::{self, dkg, primitives::{group::Share, poly}}, ed25519::PublicKey, sha256, Ed25519, Hasher, Scheme};
    use commonware_macros::test_traced;
    use commonware_p2p::simulated::{Oracle, Receiver, Sender, Link, Network};
    use commonware_runtime::{deterministic::{Context, Executor}, Clock, Metrics, Runner, Spawner};
    use futures::{channel::oneshot, future::join_all};

    use super::collector;

    type Registrations<P> = HashMap<P, ((Sender<P>, Receiver<P>), (Sender<P>, Receiver<P>))>;

    #[allow(dead_code)]
    enum Action {
        Link(Link),
        Update(Link),
        Unlink,
    }

    async fn register_validators(
        oracle: &mut Oracle<PublicKey>,
        validators: &[PublicKey],
    ) -> HashMap<PublicKey, (
        (Sender<PublicKey>, Receiver<PublicKey>),
        (Sender<PublicKey>, Receiver<PublicKey>),
    )> { 
        let mut registrations = HashMap::new();        
        for validator in validators.iter() {
            let (chunk_sender, chunk_receiver) = oracle.register(validator.clone(), 4).await.unwrap();
            let (ack_sender, ack_receiver) = oracle.register(validator.clone(), 5).await.unwrap();
            registrations.insert(validator.clone(), (
                (chunk_sender, chunk_receiver),
                (ack_sender, ack_receiver),
            ));
        }
        registrations
    }

    async fn link_validators(
        oracle: &mut Oracle<PublicKey>,
        validators: &[PublicKey],
        link: Link,
        restrict_to: Option<fn(usize, usize, usize) -> bool>,
    ) {
        for (i1, v1) in validators.iter().enumerate() {
            for (i2, v2) in validators.iter().enumerate() {
                // Ignore self
                if v2 == v1 {
                    continue;
                }

                // Restrict to certain connections
                if let Some(f) = restrict_to {
                    if !f(validators.len(), i1, i2) {
                        continue;
                    }
                }

                // Add link
                oracle
                    .add_link(v1.clone(), v2.clone(), link.clone())
                    .await
                    .unwrap();
            }
        }
    }

    async fn await_collectors(
        context: Context,
        collectors: &BTreeMap<PublicKey, collector::Mailbox<Ed25519, sha256::Digest>>,
        threshold: u64,
    ) {
        let mut receivers = Vec::new();
        for (sequencer, mailbox) in collectors.iter() {
            // Create a oneshot channel to signal when the collector has reached the threshold.
            let (tx, rx) = oneshot::channel();
            receivers.push(rx);

            // Spawn a watcher for the collector.
            context.with_label("collector_watcher").spawn({
                let sequencer = sequencer.clone();
                let mut mailbox = mailbox.clone();
                move |context| async move {
                    loop {
                        let tip = mailbox.get_tip(sequencer.clone()).await.unwrap_or(0);
                        debug!(tip, ?sequencer, "collector");
                        if tip >= threshold {
                            let _ = tx.send(sequencer.clone());
                            break;
                        }
                        context.sleep(Duration::from_millis(100)).await;
                    }
                }
            });
        }

        // Wait for all oneshot receivers to complete.
        let results = join_all(receivers).await;
        assert_eq!(results.len(), collectors.len());
    }

        async fn initialize_simulation(
        context: Context,
        num_validators: u32,
        shares_vec: &mut [Share],
    ) -> (
        Oracle<PublicKey>,
        Vec<(PublicKey, Ed25519, Share)>,
        Vec<PublicKey>,
        Registrations<PublicKey>,
    ) {
        let (network, mut oracle) = Network::new(
            context.with_label("network"),
            commonware_p2p::simulated::Config {
                max_size: 1024 * 1024,
            },
        );
        network.start();

        let mut schemes = (0..num_validators)
            .map(|i| Ed25519::from_seed(i as u64))
            .collect::<Vec<_>>();
        schemes.sort_by_key(|s| s.public_key());
        let validators: Vec<(PublicKey, Ed25519, Share)> = schemes
            .iter()
            .enumerate()
            .map(|(i, scheme)| (scheme.public_key(), scheme.clone(), shares_vec[i]))
            .collect();
        let pks = validators
            .iter()
            .map(|(pk, _, _)| pk.clone())
            .collect::<Vec<_>>();

        let registrations = register_validators(&mut oracle, &pks).await;
        let link = Link {
            latency: 10.0,
            jitter: 1.0,
            success_rate: 1.0,
        };
        link_validators(&mut oracle, &pks, link, None).await;
        (oracle, validators, pks, registrations)
    }

    fn spawn_proposer(
        context: Context,
        mailboxes: Arc<
            Mutex<BTreeMap<PublicKey, super::ingress::Mailbox<sha256::Digest, PublicKey>>>,
        >,
        invalid_when: fn(u64) -> bool,
    ) {
        context
            .clone()
            .with_label("invalid signature proposer")
            .spawn(move |context| async move {
                let mut iter = 0;
                loop {
                    iter += 1;
                    let mailbox_vec: Vec<super::ingress::Mailbox<sha256::Digest, PublicKey>> = {
                        let guard = mailboxes.lock().unwrap();
                        guard.values().cloned().collect()
                    };
                    for mut mailbox in mailbox_vec {
                        let payload = Bytes::from(format!("hello world, iter {}", iter));
                        let mut hasher = sha256::Sha256::default();
                        hasher.update(&payload);

                        // Inject an invalid digest by updating with the payload again.
                        if invalid_when(iter) {
                            hasher.update(&payload);
                        }

                        let digest = hasher.finalize();
                        mailbox.broadcast(digest).await;
                    }
                    context.sleep(Duration::from_millis(250)).await;
                }
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_validator_engines(
        context: Context,
        identity: poly::Public,
        pks: &[PublicKey],
        validators: &[(PublicKey, Ed25519, Share)],
        registrations: &mut Registrations<PublicKey>,
        mailboxes: &mut BTreeMap<PublicKey, super::ingress::Mailbox<sha256::Digest, PublicKey>>,
        collectors: &mut BTreeMap<PublicKey, super::collector::Mailbox<Ed25519, sha256::Digest>>,
        refresh_epoch_timeout: Duration,
        rebroadcast_timeout: Duration,
    ) {
        let namespace = b"my testing namespace";
        for (validator, scheme, share) in validators.iter() {
            let context = context.with_label(&validator.to_string());
            let mut coordinator = super::coordinator::Coordinator::<PublicKey>::new(
                identity.clone(),
                pks.to_vec(),
                *share,
            );
            coordinator.set_view(111);

            let (app, app_mailbox) =
                super::actor::Actor::<sha256::Digest, PublicKey>::new();
            mailboxes.insert(validator.clone(), app_mailbox.clone());

            let (collector, collector_mailbox) =
                super::collector::Collector::<Ed25519, sha256::Digest>::new(
                    namespace,
                    *poly::public(&identity),
                );
            context.with_label("collector").spawn(|_| collector.run());
            collectors.insert(validator.clone(), collector_mailbox);

            let (engine, mailbox) = Engine::new(
                context.with_label("engine"),
                Config {
                    crypto: scheme.clone(),
                    application: app_mailbox.clone(),
                    collector: collectors.get(validator).unwrap().clone(),
                    coordinator,
                    mailbox_size: 1024,
                    verify_concurrent: 1024,
                    namespace: namespace.to_vec(),
                    epoch_bounds: (1, 1),
                    height_bound: 2,
                    refresh_epoch_timeout,
                    rebroadcast_timeout,
                    journal_heights_per_section: 10,
                    journal_replay_concurrency: 1,
                    journal_name_prefix: format!("broadcast-linked-seq/{}/", validator),
                },
            );

            context.with_label("app").spawn(|_| app.run(mailbox));
            let ((a1, a2), (b1, b2)) = registrations.remove(validator).unwrap();
            engine.start((a1, a2), (b1, b2));
        }
    }

    #[test_traced]
    fn test_all_online() {
        let num_validators: u32 = 4;
        let quorum: u32 = 3;
        let (runner, mut context, _) = Executor::timed(Duration::from_secs(30));
        let (identity, mut shares_vec) = dkg::ops::generate_shares(&mut context, None, num_validators, quorum);        
        shares_vec.sort_by(|a, b| a.index.cmp(&b.index));

        runner.start(async move {
            let (_oracle, validators, pks, mut registrations) = initialize_simulation(
                context.with_label("simulation"), 
                num_validators, 
                &mut shares_vec).await;
            let mailboxes = Arc::new(Mutex::new(BTreeMap::<
                PublicKey,
                super::ingress::Mailbox<sha256::Digest, PublicKey>,
            >::new()));
            let mut collectors = BTreeMap::<PublicKey, super::collector::Mailbox<Ed25519, sha256::Digest>>::new();
            spawn_validator_engines(
                context.with_label("validator"), 
                identity.clone(), 
                &pks, 
                &validators, 
                &mut registrations, 
                &mut mailboxes.lock().unwrap(), 
                &mut collectors, 
                Duration::from_millis(100), 
                Duration::from_secs(5)
            );
            spawn_proposer(context.with_label("proposer"), mailboxes.clone(), |_| false);
            await_collectors(context.with_label("collector"), &collectors, 100).await;
        });
    }
}