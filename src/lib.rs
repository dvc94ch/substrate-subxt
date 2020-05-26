// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of substrate-subxt.
//
// subxt is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// subxt is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with substrate-subxt.  If not, see <http://www.gnu.org/licenses/>.

//! A library to **sub**mit e**xt**rinsics to a
//! [substrate](https://github.com/paritytech/substrate) node via RPC.

#![deny(
    bad_style,
    const_err,
    improper_ctypes,
    missing_docs,
    non_shorthand_field_patterns,
    no_mangle_generic_items,
    overflowing_literals,
    path_statements,
    patterns_in_fns_without_body,
    private_in_public,
    unconditional_recursion,
    unused_allocation,
    unused_comparisons,
    unused_parens,
    while_true,
    trivial_casts,
    trivial_numeric_casts,
    unused_extern_crates
)]
#![allow(clippy::type_complexity)]

#[macro_use]
extern crate substrate_subxt_proc_macro;

pub use sp_core;
pub use sp_runtime;

use codec::Encode;
use futures::future;
use jsonrpsee::client::Subscription;
use sc_rpc_api::state::ReadProof;
use sp_core::storage::{
    StorageChangeSet,
    StorageKey,
};
use sp_runtime::{
    generic::{
        SignedPayload,
        UncheckedExtrinsic,
    },
    traits::SignedExtension,
    MultiSignature,
};
use sp_version::RuntimeVersion;
use std::{
    convert::TryFrom,
    marker::PhantomData,
};

mod error;
mod events;
mod extra;
mod frame;
mod metadata;
mod rpc;
mod runtimes;
mod signer;

pub use crate::{
    error::Error,
    events::{
        EventsDecoder,
        EventsError,
        RawEvent,
    },
    extra::*,
    frame::*,
    metadata::{
        Metadata,
        MetadataError,
    },
    rpc::{
        BlockNumber,
        ExtrinsicSuccess,
    },
    runtimes::*,
    signer::*,
    substrate_subxt_proc_macro::*,
};
use crate::{
    frame::system::{
        AccountStoreExt,
        Phase,
        System,
        SystemEvent,
    },
    rpc::{
        ChainBlock,
        Rpc,
    },
};

/// ClientConfig for constructing a Client.
#[derive(Default)]
pub struct ClientConfig {
    url: Option<String>,
    client: Option<jsonrpsee::Client>,
}

impl ClientConfig {
    /// Creates a new ClientConfig.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the jsonrpsee client.
    pub fn set_client<P: Into<jsonrpsee::Client>>(mut self, client: P) -> Self {
        self.client = Some(client.into());
        self
    }

    /// Set the substrate rpc address.
    pub fn set_url<P: Into<String>>(mut self, url: P) -> Self {
        self.url = Some(url.into());
        self
    }
}

/// Client to interface with a substrate node.
pub struct Client<T: System, S = MultiSignature, E = DefaultExtra<T>> {
    rpc: Rpc<T>,
    genesis_hash: T::Hash,
    metadata: Metadata,
    runtime_version: RuntimeVersion,
    _marker: PhantomData<(fn() -> S, E)>,
}

impl<T: System, S, E> Clone for Client<T, S, E> {
    fn clone(&self) -> Self {
        Self {
            rpc: self.rpc.clone(),
            genesis_hash: self.genesis_hash,
            metadata: self.metadata.clone(),
            runtime_version: self.runtime_version.clone(),
            _marker: PhantomData,
        }
    }
}

impl<T: System, S, E> Client<T, S, E> {
    /// Creates a new client with defaults.
    pub async fn new() -> Result<Self, Error> {
        Self::with_config(Default::default()).await
    }

    /// Creates a new client from a `ClientConfig`.
    pub async fn with_config(config: ClientConfig) -> Result<Self, Error> {
        let client = if let Some(client) = config.client {
            client
        } else {
            let url = config
                .url
                .as_ref()
                .map(|s| &**s)
                .unwrap_or("ws://127.0.0.1:9944");
            if url.starts_with("ws://") || url.starts_with("wss://") {
                jsonrpsee::ws_client(url).await?
            } else {
                jsonrpsee::http_client(url)
            }
        };
        let rpc = Rpc::new(client);
        let (metadata, genesis_hash, runtime_version) = future::join3(
            rpc.metadata(),
            rpc.genesis_hash(),
            rpc.runtime_version(None),
        )
        .await;
        Ok(Self { _marker: PhantomData, rpc, genesis_hash: genesis_hash?, metadata: metadata?, runtime_version: runtime_version? })
    }

    /// Returns the chain metadata.
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// Fetch a StorageKey.
    pub async fn fetch<F: Store<T>>(
        &self,
        store: F,
        hash: Option<T::Hash>,
    ) -> Result<F::Returns, Error> {
        let key = store.key(&self.metadata)?;
        let value = self.rpc.storage::<F::Returns>(key, hash).await?;
        if let Some(v) = value {
            Ok(v)
        } else {
            Ok(store.default(&self.metadata)?)
        }
    }

    /// Query historical storage entries
    pub async fn query_storage(
        &self,
        keys: Vec<StorageKey>,
        from: T::Hash,
        to: Option<T::Hash>,
    ) -> Result<Vec<StorageChangeSet<<T as System>::Hash>>, Error> {
        self.rpc.query_storage(keys, from, to).await
    }

    /// Get a header
    pub async fn header<H>(&self, hash: Option<H>) -> Result<Option<T::Header>, Error>
    where
        H: Into<T::Hash> + 'static,
    {
        let header = self.rpc.header(hash.map(|h| h.into())).await?;
        Ok(header)
    }

    /// Get a block hash. By default returns the latest block hash
    pub async fn block_hash(
        &self,
        block_number: Option<BlockNumber<T>>,
    ) -> Result<Option<T::Hash>, Error> {
        let hash = self.rpc.block_hash(block_number).await?;
        Ok(hash)
    }

    /// Get a block hash of the latest finalized block
    pub async fn finalized_head(&self) -> Result<T::Hash, Error> {
        let head = self.rpc.finalized_head().await?;
        Ok(head)
    }

    /// Get a block
    pub async fn block<H>(&self, hash: Option<H>) -> Result<Option<ChainBlock<T>>, Error>
    where
        H: Into<T::Hash> + 'static,
    {
        let block = self.rpc.block(hash.map(|h| h.into())).await?;
        Ok(block)
    }

    /// Get proof of storage entries at a specific block's state.
    pub async fn read_proof<H>(
        &self,
        keys: Vec<StorageKey>,
        hash: Option<H>,
    ) -> Result<ReadProof<T::Hash>, Error>
    where
        H: Into<T::Hash> + 'static,
    {
        let proof = self.rpc.read_proof(keys, hash.map(|h| h.into())).await?;
        Ok(proof)
    }

    /// Subscribe to events.
    pub async fn subscribe_events(
        &self,
    ) -> Result<Subscription<StorageChangeSet<T::Hash>>, Error> {
        let events = self.rpc.subscribe_events().await?;
        Ok(events)
    }

    /// Subscribe to new blocks.
    pub async fn subscribe_blocks(&self) -> Result<Subscription<T::Header>, Error> {
        let headers = self.rpc.subscribe_blocks().await?;
        Ok(headers)
    }

    /// Subscribe to finalized blocks.
    pub async fn subscribe_finalized_blocks(
        &self,
    ) -> Result<Subscription<T::Header>, Error> {
        let headers = self.rpc.subscribe_finalized_blocks().await?;
        Ok(headers)
    }
}

impl<T, S, E> Client<T, S, E>
where
    T: System + Send + Sync + 'static,
    S: Encode + Send + Sync + 'static,
    E: SignedExtra<T> + SignedExtension + Send + Sync + 'static,
{
    /// Creates an unsigned extrinsic.
    ///
    /// If `nonce` is `None` the nonce will be fetched from the chain.
    pub async fn create_unsigned<C: Call<T>>(
        &self,
        call: C,
        account_id: &<T as System>::AccountId,
        nonce: Option<T::Index>,
    ) -> Result<SignedPayload<Encoded, <E as SignedExtra<T>>::Extra>, Error> {
        let account_nonce = if let Some(nonce) = nonce {
            nonce
        } else {
            self.account(account_id).await?.nonce
        };
        let spec_version = self.runtime_version.spec_version;
        let tx_version = self.runtime_version.transaction_version;
        let genesis_hash = self.genesis_hash;
        let call = self
            .metadata()
            .module_with_calls(C::MODULE)
            .and_then(|module| module.call(C::FUNCTION, call))?;
        let extra: E = E::new(spec_version, tx_version, account_nonce, genesis_hash);
        let raw_payload = SignedPayload::new(call, extra.extra())?;
        Ok(raw_payload)
    }

    /// Creates a signed extrinsic.
    pub async fn create_signed<C: Call<T>>(
        &self,
        call: C,
        signer: &(dyn Signer<T, S, E> + Send + Sync),
    ) -> Result<
        UncheckedExtrinsic<T::Address, Encoded, S, <E as SignedExtra<T>>::Extra>,
        Error,
    > {
        let unsigned = self
            .create_unsigned(call, signer.account_id(), signer.nonce())
            .await?;
        Ok(signer.sign(unsigned))
    }

    /// Returns an events decoder for a call.
    pub fn events_decoder<C: Call<T>>(&self) -> Result<EventsDecoder<T>, Error> {
        let metadata = self.metadata().clone();
        let mut decoder = EventsDecoder::try_from(metadata)?;
        C::events_decoder(&mut decoder)?;
        Ok(decoder)
    }

    /// Create and submit an extrinsic and return corresponding Hash if successful
    pub async fn submit_extrinsic(
        &self,
        extrinsic: UncheckedExtrinsic<
            T::Address,
            Encoded,
            S,
            <E as SignedExtra<T>>::Extra,
        >,
    ) -> Result<T::Hash, Error> {
        self.rpc.submit_extrinsic(extrinsic).await
    }

    /// Create and submit an extrinsic and return corresponding Event if successful
    pub async fn submit_and_watch_extrinsic(
        &self,
        extrinsic: UncheckedExtrinsic<
            T::Address,
            Encoded,
            S,
            <E as SignedExtra<T>>::Extra,
        >,
        decoder: EventsDecoder<T>,
    ) -> Result<ExtrinsicSuccess<T>, Error> {
        self.rpc
            .submit_and_watch_extrinsic(extrinsic, decoder)
            .await
    }

    /// Submits a transaction to the chain.
    pub async fn submit<C: Call<T>>(
        &self,
        call: C,
        signer: &(dyn Signer<T, S, E> + Send + Sync),
    ) -> Result<T::Hash, Error> {
        let extrinsic = self.create_signed(call, signer).await?;
        self.submit_extrinsic(extrinsic).await
    }

    /// Submits transaction to the chain and watch for events.
    pub async fn watch<C: Call<T>>(
        &self,
        call: C,
        signer: &(dyn Signer<T, S, E> + Send + Sync),
    ) -> Result<ExtrinsicSuccess<T>, Error> {
        let extrinsic = self.create_signed(call, signer).await?;
        let decoder = self.events_decoder::<C>()?;
        self.submit_and_watch_extrinsic(extrinsic, decoder).await
    }
}

/// Wraps an already encoded byte vector, prevents being encoded as a raw byte vector as part of
/// the transaction payload
#[derive(Clone, Debug)]
pub struct Encoded(pub Vec<u8>);

impl codec::Encode for Encoded {
    fn encode(&self) -> Vec<u8> {
        self.0.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use sp_core::{
        storage::{
            well_known_keys,
            StorageKey,
        },
        Pair,
    };
    use sp_keyring::{
        AccountKeyring,
        Ed25519Keyring,
    };

    use super::*;

    pub(crate) async fn test_client() -> Client<crate::DefaultNodeRuntime> {
        ClientBuilder::new()
            .build()
            .await
            .expect("Error creating client")
    }

    #[async_std::test]
    #[ignore] // requires locally running substrate node
    async fn test_tx_transfer_balance() {
        env_logger::try_init().ok();
        let mut signer = PairSigner::new(AccountKeyring::Alice.pair());
        let dest = AccountKeyring::Bob.to_account_id().into();

        let client = test_client().await;
        let nonce = client
            .account(&AccountKeyring::Alice.to_account_id())
            .await
            .unwrap()
            .nonce;
        signer.set_nonce(nonce);
        client
            .submit(
                balances::TransferCall {
                    to: &dest,
                    amount: 10_000,
                },
                &signer,
            )
            .await
            .unwrap();

        // check that nonce is handled correctly
        signer.increment_nonce();
        client
            .submit(
                balances::TransferCall {
                    to: &dest,
                    amount: 10_000,
                },
                &signer,
            )
            .await
            .unwrap();
    }

    #[async_std::test]
    #[ignore] // requires locally running substrate node
    async fn test_getting_hash() {
        let client = test_client().await;
        client.block_hash(None).await.unwrap();
    }

    #[async_std::test]
    #[ignore] // requires locally running substrate node
    async fn test_getting_block() {
        let client = test_client().await;
        let block_hash = client.block_hash(None).await.unwrap();
        client.block(block_hash).await.unwrap();
    }

    #[async_std::test]
    #[ignore] // requires locally running substrate node
    async fn test_getting_read_proof() {
        let client = test_client().await;
        let block_hash = client.block_hash(None).await.unwrap();
        client
            .read_proof(
                vec![
                    StorageKey(well_known_keys::HEAP_PAGES.to_vec()),
                    StorageKey(well_known_keys::EXTRINSIC_INDEX.to_vec()),
                ],
                block_hash,
            )
            .await
            .unwrap();
    }

    #[async_std::test]
    #[ignore] // requires locally running substrate node
    async fn test_chain_subscribe_blocks() {
        let client = test_client().await;
        let mut blocks = client.subscribe_blocks().await.unwrap();
        blocks.next().await;
    }

    #[async_std::test]
    #[ignore] // requires locally running substrate node
    async fn test_chain_subscribe_finalized_blocks() {
        let client = test_client().await;
        let mut blocks = client.subscribe_finalized_blocks().await.unwrap();
        blocks.next().await;
    }

    #[async_std::test]
    #[ignore] // requires locally running substrate node
    async fn test_create_raw_payload() {
        let signer_pair = Ed25519Keyring::Alice.pair();
        let signer_account_id = Ed25519Keyring::Alice.to_account_id();
        let dest = AccountKeyring::Bob.to_account_id().into();

        let client = test_client().await;

        // create raw payload with AccoundId and sign it
        let raw_payload = client
            .create_unsigned(
                balances::TransferCall {
                    to: &dest,
                    amount: 10_000,
                },
                &signer_account_id,
                None,
            )
            .await
            .unwrap();
        let raw_signature = signer_pair.sign(raw_payload.encode().as_slice());
        let raw_multisig = MultiSignature::from(raw_signature);

        // create signature with Xtbuilder
        let signer = PairSigner::new(Ed25519Keyring::Alice.pair());
        let xt_multi_sig = client
            .create_signed(
                balances::TransferCall {
                    to: &dest,
                    amount: 10_000,
                },
                &signer,
            )
            .await
            .unwrap()
            .signature
            .unwrap()
            .1;

        // compare signatures
        assert_eq!(raw_multisig, xt_multi_sig);
    }
}
