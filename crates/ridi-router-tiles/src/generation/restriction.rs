use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GenerationRestrictionRuleType {
    OnlyAllowed,
    NotAllowed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationRestrictionRule {
    pub relation_id: u64,
    pub via_node_id: u64,
    pub rule_type: GenerationRestrictionRuleType,
    pub from_line_indices: Vec<u32>,
    pub to_line_indices: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GenerationRestrictionSkipReason {
    ViaWay,
    Conditional,
    RestrictionVariant,
    Except,
    MalformedOrUnresolved,
}

impl GenerationRestrictionSkipReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::ViaWay => "via_way",
            Self::Conditional => "conditional",
            Self::RestrictionVariant => "restriction_variant",
            Self::Except => "except",
            Self::MalformedOrUnresolved => "malformed_or_unresolved",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GenerationRestrictionSkipStats {
    pub via_way_relations: u64,
    pub conditional_relations: u64,
    pub restriction_variant_relations: u64,
    pub except_relations: u64,
    pub malformed_or_unresolved_relations: u64,
}

impl GenerationRestrictionSkipStats {
    pub fn increment(&mut self, reason: GenerationRestrictionSkipReason) {
        match reason {
            GenerationRestrictionSkipReason::ViaWay => self.via_way_relations += 1,
            GenerationRestrictionSkipReason::Conditional => self.conditional_relations += 1,
            GenerationRestrictionSkipReason::RestrictionVariant => {
                self.restriction_variant_relations += 1
            }
            GenerationRestrictionSkipReason::Except => self.except_relations += 1,
            GenerationRestrictionSkipReason::MalformedOrUnresolved => {
                self.malformed_or_unresolved_relations += 1
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.via_way_relations == 0
            && self.conditional_relations == 0
            && self.restriction_variant_relations == 0
            && self.except_relations == 0
            && self.malformed_or_unresolved_relations == 0
    }

    pub fn summary_parts(&self) -> Vec<String> {
        let mut parts = Vec::new();

        if self.via_way_relations > 0 {
            parts.push(format!("via_way={}", self.via_way_relations));
        }
        if self.conditional_relations > 0 {
            parts.push(format!("conditional={}", self.conditional_relations));
        }
        if self.restriction_variant_relations > 0 {
            parts.push(format!(
                "restriction_variant={}",
                self.restriction_variant_relations
            ));
        }
        if self.except_relations > 0 {
            parts.push(format!("except={}", self.except_relations));
        }
        if self.malformed_or_unresolved_relations > 0 {
            parts.push(format!(
                "malformed_or_unresolved={}",
                self.malformed_or_unresolved_relations
            ));
        }

        parts
    }
}
