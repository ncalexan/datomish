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

//! This module implements the upsert resolution algorithm described at
//! https://github.com/mozilla/mentat/wiki/Transacting:-upsert-resolution-algorithm.

use std::collections::BTreeSet;

use mentat_tx::entities::OpType;
use errors;
use errors::ErrorKind;
use types::{Attribute, AVPair, Entid, Schema, TypedValue};
use internal_types::*;
use schema::SchemaBuilding;

/// A "Simple upsert" that looks like [:db/add TEMPID a v], where a is :db.unique/identity.
#[derive(Clone,Debug,Eq,Hash,Ord,PartialOrd,PartialEq)]
struct UpsertE(TempId, Entid, TypedValue);

/// A "Complex upsert" that looks like [:db/add TEMPID a OTHERID], where a is :db.unique/identity
#[derive(Clone,Debug,Eq,Hash,Ord,PartialOrd,PartialEq)]
struct UpsertEV(TempId, Entid, TempId);

/// A generation collects entities into populations at a single evolutionary step in the upsert
/// resolution evolution process.
///
/// The upsert resolution process is only concerned with [:db/add ...] entities until the final
/// entid allocations.  That's why we separate into special simple and complex upsert types
/// immediately, and then collect the more general term types for final resolution.
#[derive(Clone,Debug,Default,Eq,Hash,Ord,PartialOrd,PartialEq)]
pub struct Generation {
    /// "Simple upserts" that look like [:db/add TEMPID a v], where a is :db.unique/identity.
    upserts_e: Vec<UpsertE>,

    /// "Complex upserts" that look like [:db/add TEMPID a OTHERID], where a is :db.unique/identity
    upserts_ev: Vec<UpsertEV>,

    /// Entities that look like:
    /// - [:db/add TEMPID b OTHERID], where b is not :db.unique/identity;
    /// - [:db/add TEMPID b v], where b is not :db.unique/identity.
    /// - [:db/add e b OTHERID].
    allocations: Vec<TermWithTempIds>,

    /// Entities that upserted and no longer reference tempids.  These assertions are guaranteed to
    /// be in the store.
    upserted: Vec<TermWithoutTempIds>,

    /// Entities that resolved due to other upserts and no longer reference tempids.  These
    /// assertions may or may not be in the store.
    resolved: Vec<TermWithoutTempIds>,
}

#[derive(Clone,Debug,Default,Eq,Hash,Ord,PartialOrd,PartialEq)]
pub struct FinalPopulations {
    /// Upserts that upserted.
    pub upserted: Vec<TermWithoutTempIds>,

    /// Allocations that resolved due to other upserts.
    pub resolved: Vec<TermWithoutTempIds>,

    /// Allocations that required new entid allocations.
    pub allocated: Vec<TermWithoutTempIds>,
}

impl Generation {
    /// Split entities into a generation of populations that need to evolve to have their tempids
    /// resolved or allocated, and a population of inert entities that do not reference tempids.
    pub fn from<I>(terms: I, schema: &Schema) -> errors::Result<(Generation, Population)> where I: IntoIterator<Item=TermWithTempIds> {
        let mut generation = Generation::default();
        let mut inert = vec![];

        let is_unique = |a: Entid| -> errors::Result<bool> {
            let attribute: &Attribute = schema.require_attribute_for_entid(a)?;
            Ok(attribute.unique_identity)
        };

        for term in terms.into_iter() {
            match term {
                Term::AddOrRetract(op, Err(e), a, Err(v)) => {
                    if op == OpType::Add && is_unique(a)? {
                        generation.upserts_ev.push(UpsertEV(e, a, v));
                    } else {
                        generation.allocations.push(Term::AddOrRetract(op, Err(e), a, Err(v)));
                    }
                },
                Term::AddOrRetract(op, Err(e), a, Ok(v)) => {
                    if op == OpType::Add && is_unique(a)? {
                        generation.upserts_e.push(UpsertE(e, a, v));
                    } else {
                        generation.allocations.push(Term::AddOrRetract(op, Err(e), a, Ok(v)));
                    }
                },
                Term::AddOrRetract(op, Ok(e), a, Err(v)) => {
                    generation.allocations.push(Term::AddOrRetract(op, Ok(e), a, Err(v)));
                },
                Term::AddOrRetract(op, Ok(e), a, Ok(v)) => {
                    inert.push(Term::AddOrRetract(op, Ok(e), a, Ok(v)));
                },
            }
        }

        Ok((generation, inert))
    }

    /// Return true if it's possible to evolve this generation further.
    ///
    /// There can be complex upserts but no simple upserts to help resolve them.  We accept the
    /// overhead of having the database try to resolve an empty set of simple upserts, to avoid
    /// having to special case complex upserts at entid allocation time.
    pub fn can_evolve(&self) -> bool {
        !self.upserts_e.is_empty() || !self.upserts_ev.is_empty()
    }

    /// Evolve this generation one step further by rewriting the existing :db/add entities using the
    /// given temporary IDs.
    ///
    /// TODO: Considering doing this in place; the function already consumes `self`.
    pub fn evolve_one_step(self, temp_id_map: &TempIdMap) -> Generation {
        let mut next = Generation::default();

        for UpsertE(t, a, v) in self.upserts_e {
            match temp_id_map.get(&*t) {
                Some(&n) => next.upserted.push(Term::AddOrRetract(OpType::Add, n, a, v)),
                None => next.allocations.push(Term::AddOrRetract(OpType::Add, Err(t), a, Ok(v))),
            }
        }

        for UpsertEV(t1, a, t2) in self.upserts_ev {
            match (temp_id_map.get(&*t1), temp_id_map.get(&*t2)) {
                (Some(&n1), Some(&n2)) => next.resolved.push(Term::AddOrRetract(OpType::Add, n1, a, TypedValue::Ref(n2))),
                (None, Some(&n2)) => next.upserts_e.push(UpsertE(t1, a, TypedValue::Ref(n2))),
                (Some(&n1), None) => next.allocations.push(Term::AddOrRetract(OpType::Add, Ok(n1), a, Err(t2))),
                (None, None) => next.allocations.push(Term::AddOrRetract(OpType::Add, Err(t1), a, Err(t2))),
            }
        }

        // There's no particular need to separate resolved from allocations right here and right
        // now, although it is convenient.
        for term in self.allocations {
            // TODO: find an expression that destructures less?  I still expect this to be efficient
            // but it's a little verbose.
            match term {
                Term::AddOrRetract(op, Err(t1), a, Err(t2)) => {
                    match (temp_id_map.get(&*t1), temp_id_map.get(&*t2)) {
                        (Some(&n1), Some(&n2)) => next.resolved.push(Term::AddOrRetract(op, n1, a, TypedValue::Ref(n2))),
                        (None, Some(&n2)) => next.allocations.push(Term::AddOrRetract(op, Err(t1), a, Ok(TypedValue::Ref(n2)))),
                        (Some(&n1), None) => next.allocations.push(Term::AddOrRetract(op, Ok(n1), a, Err(t2))),
                        (None, None) => next.allocations.push(Term::AddOrRetract(op, Err(t1), a, Err(t2))),
                    }
                },
                Term::AddOrRetract(op, Err(t), a, Ok(v)) => {
                    match temp_id_map.get(&*t) {
                        Some(&n) => next.resolved.push(Term::AddOrRetract(op, n, a, v)),
                        None => next.allocations.push(Term::AddOrRetract(op, Err(t), a, Ok(v))),
                    }
                },
                Term::AddOrRetract(op, Ok(e), a, Err(t)) => {
                    match temp_id_map.get(&*t) {
                        Some(&n) => next.resolved.push(Term::AddOrRetract(op, e, a, TypedValue::Ref(n))),
                        None => next.allocations.push(Term::AddOrRetract(op, Ok(e), a, Err(t))),
                    }
                },
                Term::AddOrRetract(_, Ok(_), _, Ok(_)) => unreachable!(),
            }
        }

        next
    }

    // Collect id->[a v] pairs that might upsert at this evolutionary step.
    pub fn temp_id_avs<'a>(&'a self) -> Vec<(TempId, AVPair)> {
        let mut temp_id_avs: Vec<(TempId, AVPair)> = vec![];
        // TODO: map/collect.
        for &UpsertE(ref t, ref a, ref v) in &self.upserts_e {
            // TODO: figure out how to make this less expensive, i.e., don't require
            // clone() of an arbitrary value.
            temp_id_avs.push((t.clone(), (*a, v.clone())));
        }
        temp_id_avs
    }

    /// After evolution is complete, yield the set of tempids that require entid allocation.  These
    /// are the tempids that appeared in [:db/add ...] entities, but that didn't upsert to existing
    /// entids.
    pub fn temp_ids_in_allocations(&self) -> BTreeSet<TempId> {
        assert!(self.upserts_e.is_empty(), "All upserts should have been upserted, resolved, or moved to the allocated population!");
        assert!(self.upserts_ev.is_empty(), "All upserts should have been upserted, resolved, or moved to the allocated population!");

        let mut temp_ids: BTreeSet<TempId> = BTreeSet::default();

        for term in self.allocations.iter() {
            match term {
                &Term::AddOrRetract(OpType::Add, Err(ref t1), _, Err(ref t2)) => {
                    temp_ids.insert(t1.clone());
                    temp_ids.insert(t2.clone());
                },
                &Term::AddOrRetract(OpType::Add, Err(ref t), _, Ok(_)) => {
                    temp_ids.insert(t.clone());
                },
                &Term::AddOrRetract(OpType::Add, Ok(_), _, Err(ref t)) => {
                    temp_ids.insert(t.clone());
                },
                &Term::AddOrRetract(OpType::Add, Ok(_), _, Ok(_)) => unreachable!(),
                &Term::AddOrRetract(OpType::Retract, _, _, _) => {
                    // [:db/retract ...] entities never allocate entids; they have to resolve due to
                    // other upserts (or they fail the transaction).
                },
            }
        }

        temp_ids
    }

    /// After evolution is complete, use the provided allocated entids to segment `self` into
    /// populations, each with no references to tempids.
    pub fn into_final_populations(self, temp_id_map: &TempIdMap) -> errors::Result<FinalPopulations> {
        assert!(self.upserts_e.is_empty());
        assert!(self.upserts_ev.is_empty());

        let mut populations = FinalPopulations::default();

        populations.upserted = self.upserted;
        populations.resolved = self.resolved;

        for term in self.allocations {
            let allocated = match term {
                // TODO: consider require implementing require on temp_id_map.
                Term::AddOrRetract(op, Err(t1), a, Err(t2)) => {
                    match (op, temp_id_map.get(&*t1), temp_id_map.get(&*t2)) {
                        (op, Some(&n1), Some(&n2)) => Term::AddOrRetract(op, n1, a, TypedValue::Ref(n2)),
                        (OpType::Add, _, _) => unreachable!(), // This is a coding error -- every tempid in a :db/add entity should resolve or be allocated.
                        (OpType::Retract, _, _) => bail!(ErrorKind::NotYetImplemented(format!("[:db/retract ...] entity referenced tempid that did not upsert: one of {}, {}", t1, t2))),
                    }
                },
                Term::AddOrRetract(op, Err(t), a, Ok(v)) => {
                    match (op, temp_id_map.get(&*t)) {
                        (op, Some(&n)) => Term::AddOrRetract(op, n, a, v),
                        (OpType::Add, _) => unreachable!(), // This is a coding error.
                        (OpType::Retract, _) => bail!(ErrorKind::NotYetImplemented(format!("[:db/retract ...] entity referenced tempid that did not upsert: {}", t))),
                    }
                },
                Term::AddOrRetract(op, Ok(e), a, Err(t)) => {
                    match (op, temp_id_map.get(&*t)) {
                        (op, Some(&n)) => Term::AddOrRetract(op, e, a, TypedValue::Ref(n)),
                        (OpType::Add, _) => unreachable!(), // This is a coding error.
                        (OpType::Retract, _) => bail!(ErrorKind::NotYetImplemented(format!("[:db/retract ...] entity referenced tempid that did not upsert: {}", t))),
                    }
                },
                Term::AddOrRetract(_, Ok(_), _, Ok(_)) => unreachable!(), // This is a coding error -- these should not be in allocations.
            };
            populations.allocated.push(allocated);
        }

        Ok(populations)
    }
}
