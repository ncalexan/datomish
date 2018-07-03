// Copyright 2016 Mozilla
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use
// this file except in compliance with the License. You may obtain a copy of the
// License at http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software distributed
// under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
// CONDITIONS OF ANY KIND, either express or implied. See the License for the
// specific language governing permissions and limitations under the License.

#![allow(dead_code)]

use std::collections::HashMap;
use std::collections::{
    BTreeMap,
    BTreeSet,
};

extern crate mentat_core;

pub use self::mentat_core::{
    Attribute,
    AttributeBitFlags,
    DateTime,
    Entid,
    Schema,
    TypedValue,
    Utc,
    ValueType,
};

use edn::entities::{
    EntityPlace,
    TempId,
};

use errors;

/// Represents one partition of the entid space.
#[derive(Clone,Debug,Eq,Hash,Ord,PartialOrd,PartialEq)]
pub struct Partition {
    /// The first entid in the partition.
    pub start: i64,
    /// The next entid to be allocated in the partition.
    pub index: i64,
}

impl Partition {
    pub fn new(start: i64, next: i64) -> Partition {
        assert!(start <= next, "A partition represents a monotonic increasing sequence of entids.");
        Partition { start: start, index: next }
    }

    pub fn contains_entid(&self, e: i64) -> bool {
        (e >= self.start) && (e < self.index)
    }
}

/// Map partition names to `Partition` instances.
pub type PartitionMap = BTreeMap<String, Partition>;

/// Represents the metadata required to query from, or apply transactions to, a Mentat store.
///
/// See https://github.com/mozilla/mentat/wiki/Thoughts:-modeling-db-conn-in-Rust.
#[derive(Clone,Debug,Default,Eq,Hash,Ord,PartialOrd,PartialEq)]
pub struct DB {
    /// Map partition name->`Partition`.
    ///
    /// TODO: represent partitions as entids.
    pub partition_map: PartitionMap,

    /// The schema of the store.
    pub schema: Schema,
}

impl DB {
    pub fn new(partition_map: PartitionMap, schema: Schema) -> DB {
        DB {
            partition_map: partition_map,
            schema: schema
        }
    }
}

/// A pair [a v] in the store.
///
/// Used to represent lookup-refs and [TEMPID a v] upserts as they are resolved.
pub type AVPair = (Entid, TypedValue);

/// Map [a v] pairs to existing entids.
///
/// Used to resolve lookup-refs and upserts.
pub type AVMap<'a> = HashMap<&'a AVPair, Entid>;

// represents a set of entids that are correspond to attributes
pub type AttributeSet = BTreeSet<Entid>;

/// The transactor is tied to `edn::ValueAndSpan` right now, but in the future we'd like to support
/// `TypedValue` directly for programmatic use.  `TransactableValue` encapsulates the interface
/// value types (i.e., values in the value place) need to support to be transacted.
pub trait TransactableValue: Clone {
    /// Coerce this value place into the given type.  This is where we perform schema-aware
    /// coercion, for example coercing an integral value into a ref where appropriate.
    fn into_typed_value(self, schema: &Schema, value_type: ValueType) -> errors::Result<TypedValue>;

    /// Make an entity place out of this value place.  This is where we limit values in nested maps
    /// to valid entity places.
    fn into_entity_place(self) -> errors::Result<EntityPlace<Self>>;

    fn as_tempid(&self) -> Option<TempId>;
}
