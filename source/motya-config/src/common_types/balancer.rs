use std::str::FromStr;

use miette::miette;

use crate::{
    common_types::key_template::{HashOp, KeyTemplate, TransformOp},
    kdl::schema::{definitions::ValueKind, value_info::KdlValueInfo},
};

#[derive(Debug, Clone, PartialEq)]
pub struct BalancerConfig {
    pub source: KeyTemplate,
    pub fallback: Option<KeyTemplate>,
    pub algorithm: HashOp,
    pub transforms: Vec<TransformOp>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum SelectionKind {
    RoundRobin,
    Random,
    FvnHash,
    KetamaHashing,
}

impl FromStr for SelectionKind {
    type Err = miette::Report;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "RoundRobin" => Ok(SelectionKind::RoundRobin),
            "Random"     => Ok(SelectionKind::Random),
            "FNV"        => Ok(SelectionKind::FvnHash),
            "Ketama"     => Ok(SelectionKind::KetamaHashing),
            unknown => Err(miette!(
                "Unknown selection algorithm '{}'. Expected one of: 'RoundRobin', 'Random', 'FNV', 'Ketama'",
                unknown
            )),
        }
    }
}

impl KdlValueInfo for SelectionKind {
    fn value_kind() -> ValueKind {
        ValueKind::Enum(vec![
            "RoundRobin".into(),
            "Random".into(),
            "FNV".into(),
            "Ketama".into(),
        ])
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum HealthCheckKind {
    None,
}

#[derive(Debug, PartialEq, Clone)]
pub enum DiscoveryKind {
    Static,
}
