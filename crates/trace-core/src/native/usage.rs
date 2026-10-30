use super::*;
use crate::{
    accounting::{Amount, Counter, Tally},
    views::*,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UsageRole {
    Contribution { category: String },
    Reconciliation { scope: String },
    Inherited { origin: String },
    Breakdown { included_in: String },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeCounter {
    pub name: String,
    pub value: TokenCounter,
    pub source: NativeSource,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Normalization {
    pub component: String,
    pub rule: String,
    pub operands: Vec<NativeCounter>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeUsageEvidence {
    pub id: String,
    pub role: UsageRole,
    pub authority: String,
    pub accepted: bool,
    pub source: NativeSource,
    pub native_counters: Vec<NativeCounter>,
    pub normalization: Vec<Normalization>,
    pub accounting: TokenAccounting,
}
#[derive(Clone)]
pub(crate) struct UsageClaim {
    pub owner: String,
    pub amounts: [Amount; 5],
    pub reported_total: Amount,
    pub view: NativeUsageEvidence,
    pub priority: u8,
    pub partial: bool,
    pub base_role: UsageRole,
    pub ordinal: Option<u64>,
}
fn amount(v: Option<&Value>) -> Amount {
    match v {
        None => Amount::Missing,
        Some(v) => v.as_u64().map_or(Amount::Invalid, Amount::Known),
    }
}
fn arithmetic(a: &Amount, b: &Amount, subtract: bool) -> Amount {
    match (a, b) {
        (Amount::Known(a), Amount::Known(b)) => if subtract {
            a.checked_sub(*b)
        } else {
            a.checked_add(*b)
        }
        .map_or(Amount::Invalid, Amount::Known),
        (Amount::Invalid, _) | (_, Amount::Invalid) => Amount::Invalid,
        _ => Amount::Missing,
    }
}
impl UsageClaim {
    pub fn parse(
        provider: Provider,
        owner: &str,
        id: &str,
        value: &Value,
        source: NativeSource,
        role: UsageRole,
    ) -> Self {
        let paths: [&str; 6] = match provider {
            Provider::Codex => [
                "/input_tokens",
                "/output_tokens",
                "/cached_input_tokens",
                "/cache_write_input_tokens",
                "/total_tokens",
                "/reasoning_output_tokens",
            ],
            Provider::Pi => [
                "/input",
                "/output",
                "/cacheRead",
                "/cacheWrite",
                "/totalTokens",
                "/reasoning",
            ],
            Provider::Claude => [
                "/input_tokens",
                "/output_tokens",
                "/cache_read_input_tokens",
                "/cache_creation_input_tokens",
                "/total_tokens",
                "/reasoning_tokens",
            ],
            Provider::Opencode => [
                "/input",
                "/output",
                "/cache/read",
                "/cache/write",
                "/total",
                "/reasoning",
            ],
        };
        let names = [
            "input",
            "output",
            "cacheRead",
            "cacheWrite",
            "total",
            "reasoning",
        ];
        let raw = paths.map(|p| amount(value.pointer(p)));
        let mut native_counters = raw
            .iter()
            .enumerate()
            .map(|(i, a)| NativeCounter {
                name: names[i].into(),
                value: Counter::from_amount(a, false).view(),
                source: source.with_field(paths[i]),
            })
            .collect::<Vec<_>>();
        let mut normalization = Vec::new();
        let mut amounts: [Amount; 5] = std::array::from_fn(|i| raw[i].clone());
        match provider {
            Provider::Codex => {
                amounts[0] = arithmetic(&raw[0], &raw[2], true);
                normalization.push(Normalization {
                    component: "inputTokens".into(),
                    rule: "subtractCachedInputSubset".into(),
                    operands: vec![native_counters[0].clone(), native_counters[2].clone()],
                });
            }
            Provider::Opencode => {
                amounts[1] = arithmetic(&raw[1], &raw[5], false);
                normalization.push(Normalization {
                    component: "outputTokens".into(),
                    rule: "addDisjointReasoning".into(),
                    operands: vec![native_counters[1].clone(), native_counters[5].clone()],
                });
            }
            _ => {}
        }
        if matches!(provider, Provider::Codex | Provider::Pi) && raw[5] != Amount::Missing {
            normalization.push(Normalization {
                component: "outputTokens".into(),
                rule: "reasoningIsSubsetOfOutput".into(),
                operands: vec![native_counters[5].clone(), native_counters[1].clone()],
            });
        }
        let subdivisions: &[(&str, &str)] = match provider {
            Provider::Pi => &[("cacheWrite1h", "/cacheWrite1h")],
            Provider::Claude => &[
                ("cacheWrite5m", "/cache_creation/ephemeral_5m_input_tokens"),
                ("cacheWrite1h", "/cache_creation/ephemeral_1h_input_tokens"),
            ],
            _ => &[],
        };
        for (name, path) in subdivisions {
            if let Some(value) = value.pointer(path) {
                let counter = NativeCounter {
                    name: (*name).into(),
                    value: Counter::from_amount(&amount(Some(value)), false).view(),
                    source: source.with_field(path),
                };
                normalization.push(Normalization {
                    component: "cacheWriteTokens".into(),
                    rule: "cacheSubdivisionIncludedInAggregate".into(),
                    operands: vec![counter.clone(), native_counters[3].clone()],
                });
                native_counters.push(counter);
            }
        }
        if amounts[..4].iter().all(|a| matches!(a, Amount::Known(_))) {
            let total = amounts[..4]
                .iter()
                .fold(Amount::Known(0), |a, b| arithmetic(&a, b, false));
            if amounts[4] == Amount::Missing {
                amounts[4] = total;
            } else if amounts[4] != total {
                amounts[4] = Amount::Invalid;
            }
        }
        let kind = match &role {
            UsageRole::Contribution { category } if category == "modelResponse" => 4,
            UsageRole::Contribution { category } if category == "cumulativeDelta" => 16,
            _ => 8,
        };
        let accounting = Tally::contribution(&amounts, false, kind).view(false);
        Self {
            owner: owner.into(),
            base_role: role.clone(),
            ordinal: None,
            reported_total: raw[4].clone(),
            amounts,
            view: NativeUsageEvidence {
                id: id.into(),
                role,
                authority: format!("{}PinnedProfile", provider),
                accepted: false,
                source,
                native_counters,
                normalization,
                accounting,
            },
            priority: 1,
            partial: false,
        }
    }
    pub fn tally(&self) -> Tally {
        Tally::contribution(
            &self.amounts,
            self.partial,
            match &self.view.role {
                UsageRole::Contribution { category } if category == "modelResponse" => 4,
                UsageRole::Contribution { category } if category == "cumulativeDelta" => 16,
                _ => 8,
            },
        )
    }
}
#[derive(Clone, Default)]
pub(crate) struct Ledger {
    pub claims: BTreeMap<String, UsageClaim>,
}
impl Ledger {
    pub fn put(&mut self, key: String, claim: UsageClaim) {
        self.claims.insert(key, claim);
    }
    pub fn selected(&self) -> Vec<UsageClaim> {
        let mut groups: BTreeMap<&str, Vec<&UsageClaim>> = BTreeMap::new();
        for c in self.claims.values() {
            if matches!(c.view.role, UsageRole::Contribution { .. }) {
                groups.entry(&c.view.id).or_default().push(c);
            }
        }
        groups
            .into_values()
            .map(|mut claims| {
                claims.sort_by_key(|c| c.priority);
                let mut c = (*claims.pop().unwrap()).clone();
                if claims
                    .iter()
                    .any(|other| other.priority == c.priority && other.amounts != c.amounts)
                {
                    c.partial = true;
                    c.amounts = std::array::from_fn(|_| Amount::Invalid);
                }
                c.view.accepted = true;
                c
            })
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn source() -> NativeSource {
        NativeSource::Jsonl {
            provider: Provider::Pi,
            store_id: StoreId("fixture".into()),
            source: "s.jsonl".into(),
            line: "1".into(),
            byte_range: None,
        }
    }
    #[test]
    fn checked_native_normalization_and_exact_zero() {
        let c = UsageClaim::parse(
            Provider::Codex,
            "a",
            "a",
            &json!({"input_tokens":9007199254740993u64,"cached_input_tokens":3,"output_tokens":0,"cache_write_input_tokens":0}),
            source(),
            UsageRole::Contribution {
                category: "modelResponse".into(),
            },
        );
        assert_eq!(
            c.view.accounting.input_tokens.value.as_deref(),
            Some("9007199254740990")
        );
        assert_eq!(c.amounts[1], Amount::Known(0));
        assert_eq!(c.amounts[3], Amount::Known(0));
        assert_eq!(
            c.view.accounting.cache_write_tokens.value.as_deref(),
            Some("0")
        );
        let bad = UsageClaim::parse(
            Provider::Opencode,
            "a",
            "a",
            &json!({"output":u64::MAX,"reasoning":1}),
            source(),
            c.view.role,
        );
        assert_eq!(bad.amounts[1], Amount::Invalid);
        assert_eq!(bad.amounts[0], Amount::Missing);
    }
    #[test]
    fn repeated_and_noncontributing_roles() {
        let mut l = Ledger::default();
        for (key, role) in [
            (
                "a",
                UsageRole::Contribution {
                    category: "modelResponse".into(),
                },
            ),
            (
                "b",
                UsageRole::Breakdown {
                    included_in: "a".into(),
                },
            ),
            (
                "c",
                UsageRole::Inherited {
                    origin: "parent".into(),
                },
            ),
            (
                "d",
                UsageRole::Reconciliation {
                    scope: "session".into(),
                },
            ),
        ] {
            l.put(
                key.into(),
                UsageClaim::parse(
                    Provider::Pi,
                    "n",
                    "a",
                    &json!({"input":1,"output":2,"cacheRead":0,"cacheWrite":0}),
                    source(),
                    role,
                ),
            );
        }
        assert_eq!(l.selected().len(), 1);
    }
}
