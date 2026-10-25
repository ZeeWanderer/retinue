use super::*;

impl Pages {
    pub fn fixture(&mut self) {
        self.fixture = true;
        self.connected = true;
        self.requests.clear();
        self.pending.clear();
        let component = json!({"name":"Neuroptics", "full_name":"Ember Prime Neuroptics",
            "market_name":"Ember Prime Neuroptics Blueprint", "required":1, "owned":0,
            "blueprint_owned":true, "owned_relic":true, "relic_probability":0.1});
        self.inventory.replace(
            json!({"items":[
                {"id":"ember-neuroptics","name":"Ember Prime Neuroptics Blueprint","group":"parts",
                    "quantity":3,"ducats":45,"market_name":"Ember Prime Neuroptics Blueprint"},
                {"id":"ember-set","name":"Ember Prime Set","group":"sets","quantity":1,
                    "market_name":"Ember Prime Set","components":[component.clone()]},
                {"id":"hush","name":"Hush","group":"mods","quantity":30,"rank":0,"max_rank":3,
                    "market_name":"Hush"}
            ]}),
            &self.quotes,
        );
        self.mastery.replace(json!({"summary": {
            "player_level":25,"rank_progress":{"current":484500,"total":500000,"percent":96.9,"available":true},
            "content_percent":54.8,"warframes":{"mastered":60,"total":100,"percent":60.0},
            "weapons":{"mastered":360,"total":700,"percent":51.4},"companions":{"mastered":40,"total":70,"percent":57.1},
            "star_chart":{"percent":80.0,"normal":{"current":220,"total":230,"percent":95.6},"junctions":{"current":13,"total":14,"percent":92.8},
                "steel":{"current":180,"total":230,"percent":78.2},"steel_junctions":{"current":10,"total":14,"percent":71.4}},
            "intrinsics":{"percent":75.0,"railjack":{"current":40,"total":50,"percent":80.0},"duviri":{"current":30,"total":40,"percent":75.0}}
        },"items":[{"id":"ember-prime","name":"Ember Prime","group":"warframes","owned":true,
            "rank":18,"max_rank":30,"potential_xp":2400,"earned_xp":3600,"has_recipe":true,
            "from_relics":true,"buyable":true,"components":[component]}]}), &self.quotes);
        self.relics.replace(json!({"items":[{"id":"lith-a1","name":"Lith A1","era":"Lith",
            "amount_owned":4,"refinements":[{"refinement":"Intact","amount_owned":3,"expected_ducats":18},
                {"refinement":"Radiant","amount_owned":1,"expected_ducats":35}],
            "rewards":[{"name":"Ember Prime Neuroptics Blueprint","slug":"ember_prime_neuroptics_blueprint","rarity":"Rare","chance":2.0,"ducats":45,"owned":1},
                {"name":"Forma Blueprint","rarity":"Common","chance":25.33,"ducats":0,"owned":12}]}]}), &self.quotes);
        self.quotes.insert(
            "Ember Prime Neuroptics Blueprint".into(),
            Quote {
                sell: Some(15.0),
                buy: Some(9.0),
                resolved: true,
            },
        );
        self.quotes.insert(
            "Ember Prime Set".into(),
            Quote {
                sell: Some(90.0),
                buy: Some(70.0),
                resolved: true,
            },
        );
        self.market.account = json!({"authenticated":true,"profile":{"ingameName":"Tenno"},"orders":[
            {"id":"example","itemId":"ember-neuroptics","type":"sell","quantity":3,"platinum":15,"visible":true}]});
        self.market.catalog.insert("ember-neuroptics".into(),json!({"id":"ember-neuroptics","name":"Ember Prime Neuroptics Blueprint","group":"parts","slug":"ember_prime_neuroptics_blueprint"}));
        self.builds.fixture();
        self.builds.account = json!({"authenticated":false});
        self.activity = json!({"fissures":[{"id":"example","tier":"Lith","mission":"Survival","node":"Teshub (Void)","hard":false,"expiry":unix_ms()+600000}]});
        self.notifications = json!({"fissures":{"mode":"session"}});
        self.resort(true);
        self.dirty = true;
        self.layout();
    }
}
