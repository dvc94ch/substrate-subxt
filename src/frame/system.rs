// Copyright 2019 Parity Technologies (UK) Ltd.
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

//! Implements support for the frame_system module.

use codec::{
    Codec,
    Decode,
    Encode,
};
use frame_support::Parameter;
use serde::de::DeserializeOwned;
use sp_core::storage::StorageKey;
use sp_runtime::{
    traits::{
        AtLeast32Bit,
        Bounded,
        CheckEqual,
        Extrinsic,
        Hash,
        Header,
        MaybeDisplay,
        MaybeMallocSizeOf,
        MaybeSerialize,
        MaybeSerializeDeserialize,
        Member,
        SimpleBitOps,
    },
    RuntimeDebug,
};
use std::fmt::Debug;

use crate::{
    frame::{Call, Store},
    metadata::{Metadata, MetadataError},
};

/// The subset of the `frame::Trait` that a client must implement.
pub trait System: 'static + Eq + Clone + Debug {
    /// Account index (aka nonce) type. This stores the number of previous
    /// transactions associated with a sender account.
    type Index: Parameter
        + Member
        + MaybeSerialize
        + Debug
        + Default
        + MaybeDisplay
        + AtLeast32Bit
        + Copy;

    /// The block number type used by the runtime.
    type BlockNumber: Parameter
        + Member
        + MaybeMallocSizeOf
        + MaybeSerializeDeserialize
        + Debug
        + MaybeDisplay
        + AtLeast32Bit
        + Default
        + Bounded
        + Copy
        + std::hash::Hash
        + std::str::FromStr;

    /// The output of the `Hashing` function.
    type Hash: Parameter
        + Member
        + MaybeMallocSizeOf
        + MaybeSerializeDeserialize
        + Debug
        + MaybeDisplay
        + Ord
        + SimpleBitOps
        + Default
        + Copy
        + CheckEqual
        + std::hash::Hash
        + AsRef<[u8]>
        + AsMut<[u8]>;

    /// The hashing system (algorithm) being used in the runtime (e.g. Blake2).
    type Hashing: Hash<Output = Self::Hash>;

    /// The user account identifier type for the runtime.
    type AccountId: Parameter
        + Member
        + MaybeSerialize
        + Debug
        + MaybeDisplay
        + Ord
        + Default;

    /// The address type. This instead of `<frame_system::Trait::Lookup as StaticLookup>::Source`.
    type Address: Codec + Clone + PartialEq + Debug + Send + Sync;

    /// The block header.
    type Header: Parameter
        + Header<Number = Self::BlockNumber, Hash = Self::Hash>
        + DeserializeOwned;

    /// Extrinsic type within blocks.
    type Extrinsic: Parameter + Member + Extrinsic + Debug + MaybeSerializeDeserialize;

    /// Data to be associated with an account (other than nonce/transaction counter, which this
    /// module does regardless).
    type AccountData: Member + Codec + Clone + Default;
}

/// Type used to encode the number of references an account has.
pub type RefCount = u8;

/// Information of an account.
#[derive(Clone, Eq, PartialEq, Default, RuntimeDebug, Encode, Decode)]
pub struct AccountInfo<T: System> {
    /// The number of transactions this account has sent.
    pub nonce: T::Index,
    /// The number of other modules that currently depend on this account's existence. The account
    /// cannot be reaped until this is zero.
    pub refcount: RefCount,
    /// The additional data that belongs to this account. Used to store the balance(s) in a lot of
    /// chains.
    pub data: T::AccountData,
}

const MODULE: &str = "System";

/// Account field of the `System` module.
#[derive(Encode)]
pub struct AccountStore<'a, T: System>(pub &'a T::AccountId);

impl<'a, T: System> Store<T> for AccountStore<'a, T> {
    const MODULE: &'static str = MODULE;
    const FIELD: &'static str = "Account";
    type Returns = AccountInfo<T>;

    fn key(&self, metadata: &Metadata) -> Result<StorageKey, MetadataError> {
        Ok(metadata
            .module(Self::MODULE)?
            .storage(Self::FIELD)?
            .map()?
            .key(self.0))
    }
}

/// Arguments for updating the runtime code
#[derive(Encode)]
pub struct SetCodeCall<'a>(pub &'a Vec<u8>);

impl<'a, T: System> Call<T> for SetCodeCall<'a> {
    const MODULE: &'static str = MODULE;
    const FUNCTION: &'static str = "set_code";
}

use frame_support::weights::DispatchInfo;

/// Event for the System module.
#[derive(Clone, Debug, codec::Decode)]
pub enum SystemEvent<T: System> {
    /// An extrinsic completed successfully.
    ExtrinsicSuccess(DispatchInfo),
    /// An extrinsic failed.
    ExtrinsicFailed(sp_runtime::DispatchError, DispatchInfo),
    /// `:code` was updated.
    CodeUpdated,
    /// A new account was created.
    NewAccount(T::AccountId),
    /// An account was reaped.
    ReapedAccount(T::AccountId),
}

/// A phase of a block's execution.
#[derive(codec::Decode)]
pub enum Phase {
    /// Applying an extrinsic.
    ApplyExtrinsic(u32),
    /// The end.
    Finalization,
}
