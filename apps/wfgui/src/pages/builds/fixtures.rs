use super::*;

impl Builds {
    pub(in crate::pages) fn fixture(&mut self) {
        let mods=(1..=8).map(|i|json!({"id":format!("mod-{i}"),"label":format!("Mod {i}"),"role":"mod","planner":true})).collect::<Vec<_>>();
        let topology = json!({"regions":[
            {"id":"special","label":"Special slots","columns":4,"slots":[{"id":"exilus","label":"Exilus","role":"exilus","unlocked":false,"planner":true}]},
            {"id":"mods","label":"Mods","columns":4,"slots":mods},
            {"id":"arcanes","label":"Arcanes","columns":4,"slots":[{"id":"arcane-1","label":"Arcane 1","role":"arcane","unlocked":true,"planner":false}]}
        ]});
        let upgrades = json!([
            {"topology_slot":"mod-1","name":"Galvanized Hell","rank":10,"max_rank":10,"mod_variant":"galvanized","rarity":"legendary","polarity":"madurai","polarity_state":"matched","effective_drain":8,"effects":["+110% Multishot","On Kill: +30% Multishot for 20s."]},
            {"topology_slot":"mod-2","name":"Primed Point Blank","rank":10,"max_rank":10,"rarity":"legendary","polarity":"madurai","polarity_state":"matched","effective_drain":7,"effects":["+165% Damage"]},
            {"topology_slot":"arcane-1","name":"Primary Merciless","rank":5,"max_rank":5,"rarity":"rare"}
        ]);
        let polarities = json!([{"slot_id":"mod-1","polarity":"madurai"},{"slot_id":"mod-2","polarity":"madurai"}]);
        let config = json!({"config_index":0,"name":"Config A","upgrade_slots":upgrades,"ability_override":[]});
        self.equipment = json!({"definitions":[{"id":"boar-prime","name":"Boar Prime","class":"primary","topology":topology}],
            "instances":[{"instance_id":"owned-boar","definition_id":"boar-prime","topology":topology,"effective_polarities":polarities,"configs":[config],"forma_count":3,"shard_slots":[]}]});
        self.instance = "owned-boar".into();
        self.definition = "boar-prime".into();
        self.detail = json!({"identity":{"source":"overframe","external_id":100},"fingerprint":"fixture-source",
            "metadata":{"title":"Boar Prime - Status build","item":{"canonical_id":"boar-prime","name":"Boar Prime"},"description":"## Build notes\n\nUse **Galvanized Hell** for multishot.\n\n- Keep elemental pairs intact.\n- [Build source](/build/100)","url":"/build/100"},
            "presentation":{"topology":topology,"upgrades":upgrades},"content":{}});
        self.group = json!({"id":"fixture-group","name":"Boar Prime","definition_id":"boar-prime","instance_id":"owned-boar","revision":3,
            "baseline":{"topology":topology,"effective_polarities":polarities,"shard_slots":[]},
            "options":{"keep_mod_positions":false},"members":[{"id":"fixture-member","kind":"source_revision","snapshot":self.detail,"name":"Boar Prime - Status build"}],
            "plan_result":{"status":"ready","forma_count":0,"forma_requirements":{},"operations":[],
                "final_polarities":[{"slot_id":"mod-1","label":"Mod 1","role":"mod","before":"madurai","polarity":"madurai","changed":false},{"slot_id":"mod-2","label":"Mod 2","role":"mod","before":"madurai","polarity":"madurai","changed":false}],
                "builds":[{"member_id":"fixture-member","name":"Boar Prime - Status build","capacity":60,"drain":15,"remaining_capacity":45,"upgrade_slots":upgrades}]}});
        self.groups = json!({"groups":[self.group]});
    }
}
