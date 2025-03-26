pub mod actor;
pub mod ingress;
pub mod coordinator;
pub mod collector;
pub mod mempool;
pub mod handler;
pub mod key;
pub mod archive;

#[cfg(test)]
mod tests {
    use core::panic;
    use std::{collections::{BTreeMap, HashMap}, num::NonZeroU32, sync::{Arc, Mutex}, time::Duration};
    use bytes::Bytes;
    use commonware_broadcast::linked::{Config, Engine};
    
    use governor::Quota;
    use tracing::{debug, info, warn};

    use commonware_cryptography::{bls12381::{dkg, primitives::{group::Share, poly}}, ed25519::PublicKey, sha256, Ed25519, Scheme};
    use commonware_macros::test_traced;
    use commonware_p2p::simulated::{Oracle, Receiver, Sender, Link, Network};
    use commonware_runtime::{deterministic::{Context, Executor}, Clock, Metrics, Runner, Spawner};
    use futures::channel::mpsc;

    use super::{ingress, mempool::{self, Mempool, RawTransaction}};

    type Registrations<P> = HashMap<P, (
        (Sender<P>, Receiver<P>), 
        (Sender<P>, Receiver<P>), 
        (Sender<P>, Receiver<P>),
        (Sender<P>, Receiver<P>)
    )>;

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
        (Sender<PublicKey>, Receiver<PublicKey>),
        (Sender<PublicKey>, Receiver<PublicKey>),
    )> { 
        let mut registrations = HashMap::new();        
        for validator in validators.iter() {
            let (digest_sender, digest_receiver) = oracle.register(validator.clone(), 4).await.unwrap();
            let (ack_sender, ack_receiver) = oracle.register(validator.clone(), 5).await.unwrap();
            let (batch_sender, batch_receiver) = oracle.register(validator.clone(), 6).await.unwrap();
            let (batch_backfill_sender, batch_backfill_receiver) = oracle.register(validator.clone(), 7).await.unwrap();
            registrations.insert(validator.clone(), (
                (digest_sender, digest_receiver),
                (ack_sender, ack_receiver),
                (batch_sender, batch_receiver),
                (batch_backfill_sender, batch_backfill_receiver),
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

    #[allow(clippy::too_many_arguments)]
    async fn spawn_validator_engines(
        context: Context,
        identity: poly::Public,
        pks: &[PublicKey],
        validators: &[(PublicKey, Ed25519, Share)],
        registrations: &mut Registrations<PublicKey>,
        collectors: &mut BTreeMap<PublicKey, super::collector::Mailbox<Ed25519, sha256::Digest>>,
        refresh_epoch_timeout: Duration,
        rebroadcast_timeout: Duration,
    ) -> BTreeMap<PublicKey, mempool::Mailbox<sha256::Digest>> {
        let mut mailboxes = BTreeMap::new();
        let namespace = b"my testing namespace";
        for (validator, scheme, share) in validators.iter() {
            let (mempool, mempool_mailbox) = Mempool::init(context.with_label("mempool"), mempool::Config { 
                batch_propose_interval: Duration::from_millis(500), 
                batch_size_limit: 1024*1024, 
                backfill_quota: Quota::per_second(NonZeroU32::new(10).unwrap()),
                mailbox_size: 1024,
                public_key: scheme.public_key(),
                block_height: 0,
                partition_prefix: format!("mempool"),
            }).await;
            mailboxes.insert(validator.clone(), mempool_mailbox.clone());


            let context = context.with_label(&validator.to_string());
            let mut coordinator = super::coordinator::Coordinator::<PublicKey>::new(
                identity.clone(),
                pks.to_vec(),
                *share,
            );
            coordinator.set_view(111);

            let (app, app_mailbox) =
                super::actor::Actor::<sha256::Digest, PublicKey>::new();

            let collector_mempool_mailbox = mempool_mailbox.clone();
            let (collector, collector_mailbox) =
                super::collector::Collector::<Ed25519, sha256::Digest>::new(
                    namespace,
                    *poly::public(&identity),
                );
            context.with_label("collector").spawn(move |_| collector.run(collector_mempool_mailbox));
            collectors.insert(validator.clone(), collector_mailbox);

            let (engine, mailbox) = Engine::new(
                context.with_label("engine"),
                Config {
                    crypto: scheme.clone(),
                    application: app_mailbox.clone(),
                    collector: collectors.get(validator).unwrap().clone(),
                    coordinator: coordinator.clone(),
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

            context.with_label("app").spawn(move |_| app.run(mailbox, mempool_mailbox));
            let ((a1, a2), (b1, b2), (c1, c2), (d1, d2)) = registrations.remove(validator).unwrap();
            engine.start((a1, a2), (b1, b2));
            mempool.start((c1, c2), (d1, d2), coordinator, app_mailbox.clone());
        }
        mailboxes
    }

    async fn spawn_mempools(
        context: Context,
        identity: poly::Public,
        pks: &[PublicKey],
        validators: &[(PublicKey, Ed25519, Share)],
        registrations: &mut Registrations<PublicKey>,
        app_mailbox: &mut ingress::Mailbox<sha256::Digest, PublicKey>,
        // mailboxes: &mut BTreeMap<PublicKey, mempool::Mailbox<sha256::Digest>>,
    ) -> BTreeMap<PublicKey, mempool::Mailbox<sha256::Digest>> {
        let mut mailboxes= BTreeMap::new();
        for (validator, _, share) in validators.iter() {
            let context = context.with_label(&validator.to_string());
            let mut coordinator = super::coordinator::Coordinator::<PublicKey>::new(
                identity.clone(),
                pks.to_vec(),
                *share,
            );
            coordinator.set_view(111);

            let (mempool, mailbox) = Mempool::init(
                context.with_label("mempool"), 
                mempool::Config {
                    batch_propose_interval: Duration::from_millis(500), 
                    batch_size_limit: 1024*1024, 
                    backfill_quota: Quota::per_second(NonZeroU32::new(10).unwrap()),
                    mailbox_size: 1024,
                    public_key: validator.clone(),
                    block_height: 0,
                    partition_prefix: format!("mempool"),
                }
            ).await;
            mailboxes.insert(validator.clone(), mailbox);
            let ((_, _), (_, _), (c1, c2), (d1, d2)) = registrations.remove(validator).unwrap();
            mempool.start((c1, c2), (d1, d2),coordinator, app_mailbox.clone());
        }

        mailboxes
    }

    async fn spawn_tx_issuer_and_wait(
        context: Context,
        mailboxes: Arc<Mutex<BTreeMap<PublicKey, mempool::Mailbox<sha256::Digest>>>>,
        num_txs: u32,
        wait_batch_acknowlegement: bool,
        consume_batch: bool,
    ) {
        context
            .clone()
            .with_label("tx issuer")
            .spawn(move |context| async move {
                let mut mailbox_vec: Vec<mempool::Mailbox<sha256::Digest>> = {
                    let guard = mailboxes.lock().unwrap();
                    guard.values().cloned().collect()
                };

                if mailbox_vec.len() <= 1 {
                    panic!("insuffient mempool nodes spawned, have {}", mailbox_vec.len());
                }

                let Some(mut mailbox)= mailbox_vec.pop() else {
                    panic!("no single mailbox provided");
                };

                
                // issue tx to the first validator
                let mut digests = Vec::new();
                for i in 0..num_txs {
                    let tx = RawTransaction::new(Bytes::from(format!("tx-{}", i)));
                    let submission_res = mailbox.issue_tx(tx.clone()).await;
                    if !submission_res {
                        warn!(?tx.digest, "failed to submit tx");
                        continue;
                    }
                    debug!("tx submitted: {}", tx.digest);
                    digests.push(tx.digest);
                }

                if digests.len() == 0 {
                    panic!("zero txs issued");
                }

                context.sleep(Duration::from_secs(5)).await;

                // check if the tx appear in other validators
                for mut mailbox in mailbox_vec {
                    for digest in digests.iter() {
                        let Some(tx) = mailbox.get_tx(digest.clone()).await else {
                            panic!("digest: {} not found at mailbox", digest);
                        };

                        info!("tx found at mempool: {}", tx.digest);

                        if wait_batch_acknowlegement {
                            let Some(batch) = mailbox.get_batch_contain_tx(digest.clone()).await else {
                                panic!("batch not found");
                            };
                            if !batch.accepted {
                                panic!("batch {} not acknowledged", batch.digest);
                            }
                            info!("batch contain tx {} acknowledged", batch.digest);
                        }
                        if consume_batch {
                            // 1. consume the batches
                            let batches = mailbox.consume_batches().await;
                            assert!(batches.len() != 0, "expected some batches to consume");
                            info!("consumed batches: {:?}", batches);
                            // 2. mark the batches as consumed
                            info!("marking batches as consumed");
                            let digests = batches.iter().map(|batch| batch.digest).collect();
                            let _ = mailbox.consumed_batches(digests, 0).await;
                            // 3. try consume batches again
                            let batches = mailbox.consume_batches().await;
                            assert!(batches.len() == 0, "expected zero batches to consume");
                            info!("zero batches left after consumption");
                        }
                    }
                }
            }).await.unwrap();
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
            // let mailboxes = Arc::new(Mutex::new(BTreeMap::<
            //     PublicKey,
            //     mempool::Mailbox<sha256::Digest>,
            // >::new()));
            let mut collectors = BTreeMap::<PublicKey, super::collector::Mailbox<Ed25519, sha256::Digest>>::new();
            let mailboxes = spawn_validator_engines(
                context.with_label("validator"), 
                identity.clone(), 
                &pks, 
                &validators, 
                &mut registrations, 
                &mut collectors, 
                Duration::from_millis(100), 
                Duration::from_secs(5)
            ).await;
            let guarded_mailboxes = Arc::new(Mutex::new(mailboxes));
            spawn_tx_issuer_and_wait(context.with_label("tx_issuer"), guarded_mailboxes, 1, true, true).await;
        });
    }

    #[test_traced]
    fn test_mempool_p2p() {
        let num_validators: u32 = 4;
        let quorum: u32 = 3;
        let (runner, mut context, _) = Executor::timed(Duration::from_secs(30));
        let (identity, mut shares_vec) = dkg::ops::generate_shares(&mut context, None, num_validators, quorum);        
        shares_vec.sort_by(|a, b| a.index.cmp(&b.index));

        info!("mempool p2p test started");
        let (app_mailbox_sender, _) = mpsc::channel(1024);
        let mut app_mailbox = ingress::Mailbox::new(app_mailbox_sender);

        runner.start(async move {
            let (_oracle, validators, pks, mut registrations ) = initialize_simulation(
                context.with_label("simulation"), 
                num_validators, 
                &mut shares_vec).await;
            // let mailboxes = Arc::new(Mutex::new(BTreeMap::<
            //     PublicKey,
            //     mempool::Mailbox<sha256::Digest>,
            // >::new()));
            let mailboxes = spawn_mempools(
                context.with_label("mempool"), 
                identity,
                &pks, 
                &validators, 
                &mut registrations, 
                &mut app_mailbox, 
            ).await;
            let guarded_mailboxes = Arc::new(Mutex::new(mailboxes));
            spawn_tx_issuer_and_wait(context.with_label("tx_issuer"), guarded_mailboxes, 1, false, false).await;
        });
    }
}