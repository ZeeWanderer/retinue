%%%-------------------------------------------------------------------
%% Terminal block layouts for export entities.
%%%-------------------------------------------------------------------
-module(wfcli_exports_presentation).

-export([block_spec/1]).

-doc "Return terminal block layout for one export result kind.".
-spec block_spec(mod | item) -> map().
block_spec(mod) ->
    #{title => "Mod: {name}",
      fields => [{"Type", type}, {"Polarity", polarity}, {"Rarity", rarity},
                 {"Drain", baseDrain}, {"Fusion", fusionLimit}, {"Compat", compatName},
                 {"Description", description}, {"Effects", effects},
                 {"Max stats", max_stats}, {"Unique", uniqueName}]};
block_spec(item) ->
    #{title => "Item: {name}",
      fields => [{"File", file}, {"Category", productCategory}, {"MR", masteryReq},
                 {"Damage", totalDamage}, {"Crit%", criticalChance}, {"Critx", criticalMultiplier},
                 {"Status%", procChance}, {"Armor", armor}, {"Health", health}, {"Shield", shield},
                 {"Sprint", sprintSpeed}, {"Build time", buildTime}, {"Build price", buildPrice},
                 {"Skip price", skipBuildTimePrice}, {"Result", resultType},
                 {"Ingredients", ingredientsCount}, {"Secret", secretIngredientsCount},
                 {"Count", resultCount}, {"Consume", consumeOnUse}, {"Trigger", trigger},
                 {"Fire rate", fireRate}, {"Mag", magazineSize}, {"Reload", reloadTime},
                 {"Accuracy", accuracy}, {"Noise", noise}, {"Dmg/shot", damagePerShot},
                 {"Abilities", abilitiesCount}, {"Ability names", abilities}, {"Unique", uniqueName}]}.
