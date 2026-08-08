//! Generated contract bindings.
//!
//! Keep this file deterministic. The schema manifest below is regenerated from
//! the JSON schemas by the native regeneration-diff test in `lib.rs`.

use serde::{Deserialize, Serialize};

/// The schema inputs used to produce these bindings, in stable order.
pub const SCHEMA_MANIFEST: &str = concat!(
    "parchmint.annotation-sidecar/v1\t1\ta717f10bd68211181d6266dd2f2e238562792f23431251ed268f97a959bd7869\t",
    "properties=document_id,schema,threads\trequired=schema,document_id,threads\n",
    "parchmint.cli-output/v1\t1\t3e7e51781b958997f892e3774424beeeb1ef1d997f0ae81ab9b807511df5d212\t",
    "properties=data,message,ok,schema\trequired=schema,ok\n",
    "parchmint.recovery-record/v1\t1\tbbee00b3c3260714eb78fee50990c157663adf48496bd34a560fc92d62aacc59\t",
    "properties=operations,record_id,schema\trequired=schema,record_id,operations\n",
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationSidecarV1 {
    pub schema: String,
    pub document_id: String,
    pub threads: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryRecordV1 {
    pub schema: String,
    pub record_id: String,
    pub operations: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CliOutputV1 {
    pub schema: String,
    pub ok: bool,
    pub message: Option<String>,
    pub data: Option<serde_json::Value>,
}
