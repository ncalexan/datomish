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

use rusqlite;

use uuid;

use std::collections::BTreeSet;
use std::path::PathBuf;

use edn;
use mentat_core::{
    Attribute,
    ValueType,
};
use mentat_db;
use mentat_query;
use mentat_query_algebrizer;
use mentat_query_parser;
use mentat_query_projector;
use mentat_query_pull;
use mentat_query_translator;
use mentat_sql;
use mentat_tolstoy;

error_chain! {
    types {
        Error, ErrorKind, ResultExt, Result;
    }

    foreign_links {
        EdnParseError(edn::ParseError);
        Rusqlite(rusqlite::Error);
        UuidParseError(uuid::ParseError);
        IoError(::std::io::Error);
    }

    links {
        DbError(mentat_db::Error, mentat_db::ErrorKind);
        QueryError(mentat_query_algebrizer::Error, mentat_query_algebrizer::ErrorKind);   // Let's not leak the term 'algebrizer'.
        QueryParseError(mentat_query_parser::Error, mentat_query_parser::ErrorKind);
        ProjectorError(mentat_query_projector::errors::Error, mentat_query_projector::errors::ErrorKind);
        PullError(mentat_query_pull::errors::Error, mentat_query_pull::errors::ErrorKind);
        TranslatorError(mentat_query_translator::Error, mentat_query_translator::ErrorKind);
        SqlError(mentat_sql::Error, mentat_sql::ErrorKind);
        SyncError(mentat_tolstoy::Error, mentat_tolstoy::ErrorKind);
    }

    errors {
        PathAlreadyExists(path: String) {
            description("path already exists")
            display("path {} already exists", path)
        }

        UnboundVariables(names: BTreeSet<String>) {
            description("unbound variables at query execution time")
            display("variables {:?} unbound at query execution time", names)
        }

        InvalidArgumentName(name: String) {
            description("invalid argument name")
            display("invalid argument name: '{}'", name)
        }

        UnknownAttribute(name: String) {
            description("unknown attribute")
            display("unknown attribute: '{}'", name)
        }

        InvalidVocabularyVersion {
            description("invalid vocabulary version")
            display("invalid vocabulary version")
        }

        ConflictingAttributeDefinitions(vocabulary: String, version: ::vocabulary::Version, attribute: String, current: Attribute, requested: Attribute) {
            description("conflicting attribute definitions")
            display("vocabulary {}/{} already has attribute {}, and the requested definition differs", vocabulary, version, attribute)
        }

        ExistingVocabularyTooNew(name: String, existing: ::vocabulary::Version, ours: ::vocabulary::Version) {
            description("existing vocabulary too new")
            display("existing vocabulary too new: wanted {}, got {}", ours, existing)
        }

        UnexpectedCoreSchema(version: Option<::vocabulary::Version>) {
            description("unexpected core schema version")
            display("core schema: wanted {}, got {:?}", mentat_db::CORE_SCHEMA_VERSION, version)
        }

        MissingCoreVocabulary(kw: mentat_query::Keyword) {
            description("missing core vocabulary")
            display("missing core attribute {}", kw)
        }

        PreparedQuerySchemaMismatch {
            description("schema changed since query was prepared")
            display("schema changed since query was prepared")
        }

        ValueTypeMismatch(provided: ValueType, expected: ValueType) {
            description("provided value doesn't match value type")
            display("provided value of type {} doesn't match attribute value type {}", provided, expected)
        }

        StoreNotFound(path: String) {
            description("the Store provided does not exist or is not yet open.")
            display("the Store at {:?} does not exist or is not yet open.", path)
        }

        StorePathMismatch(name: String, actual: PathBuf, expected: PathBuf) {
            description("Path provided does not match expected path")
            display("Cannot open store {:?} at path {:?} as it does not match previous store location {:?}", name, actual.to_str(), expected.to_str())
        }

        StoreConnectionStillActive(path: String) {
            description("the Store provided has active connections and cannot be closed.")
            display("the Store at {:?} has active connections and cannot be closed.", path)
        }
    }
}
