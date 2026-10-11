%%%-------------------------------------------------------------------
%% Shared export result-column and polarity contract.
%%%-------------------------------------------------------------------
-module(wfcli_exports_schema).

-export([columns_spec/0, column_spec/1, columns_for_kind/1,
         polarity_symbol/1, polarity_display/1]).

-type kind() :: mod | item.
-type column() :: atom() | {extra, term()}.
-type column_spec() :: map().

-spec polarity_symbol(term()) -> string().
polarity_symbol(Polarity) ->
    case string:uppercase(retinue_text:to_list(Polarity)) of
        "AP_ATTACK" -> "V";
        "AP_DEFENSE" -> "D";
        "AP_TACTIC" -> "-";
        "AP_POWER" -> "=";
        "AP_WARD" -> "Y";
        "AP_UMBRA" -> "U";
        "AP_PRECEPT" -> "P";
        _ -> ""
    end.

-spec polarity_display(term()) -> string().
polarity_display(Polarity) ->
    case polarity_symbol(Polarity) of
        "" -> retinue_text:to_list(Polarity);
        Symbol -> Symbol ++ " (" ++ retinue_text:to_list(Polarity) ++ ")"
    end.

-spec column_spec(column()) -> column_spec().
column_spec({extra, Key}) ->
    #{label => Key, role => extra, optional => true};
column_spec(Col) ->
    case lists:dropwhile(fun(Spec) -> maps:get(key, Spec, undefined) =/= Col end, columns_spec()) of
        [] -> #{key => Col, label => atom_to_list(Col)};
        [Spec | _] -> Spec
    end.

-spec columns_for_kind(kind()) -> [atom()].
columns_for_kind(mod) ->
    [name, type, polarity, rarity, baseDrain, fusionLimit, compatName, effects];
columns_for_kind(item) ->
    [name, file, productCategory, masteryReq, totalDamage, criticalChance, criticalMultiplier, procChance,
     armor, health, shield, sprintSpeed, buildTime, buildPrice, skipBuildTimePrice, resultType,
     ingredientsCount, secretIngredientsCount, resultCount, consumeOnUse,
     trigger, fireRate, magazineSize, reloadTime, accuracy, noise, damagePerShot,
     abilitiesCount, abilities, uniqueName].

-spec columns_spec() -> [column_spec()].
columns_spec() ->
    [
        #{key => name, label => "Name", role => name},
        #{key => type, label => "Type", role => type},
        #{key => polarity, label => "Pol", role => stat},
        #{key => rarity, label => "Rarity", role => stat},
        #{key => baseDrain, label => "Drain", role => stat},
        #{key => fusionLimit, label => "Fusion", role => stat},
        #{key => compatName, label => "Compat", role => stat},
        #{key => effects, label => "Effects", role => details, optional => true},
        #{key => file, label => "File", role => details, optional => true},
        #{key => productCategory, label => "Category", role => stat},
        #{key => masteryReq, label => "MR", role => stat},
        #{key => totalDamage, label => "Damage", role => stat},
        #{key => criticalChance, label => "Crit%", role => stat},
        #{key => criticalMultiplier, label => "Critx", role => stat},
        #{key => procChance, label => "Status%", role => stat},
        #{key => armor, label => "Armor", role => stat},
        #{key => health, label => "Health", role => stat},
        #{key => shield, label => "Shield", role => stat},
        #{key => sprintSpeed, label => "Sprint", role => stat},
        #{key => buildTime, label => "Build time", role => stat},
        #{key => buildPrice, label => "Build price", role => stat},
        #{key => skipBuildTimePrice, label => "Skip price", role => stat},
        #{key => resultType, label => "Result", role => details, optional => true},
        #{key => ingredientsCount, label => "Ingredients", role => stat, optional => true},
        #{key => secretIngredientsCount, label => "Secret", role => stat, optional => true},
        #{key => resultCount, label => "Count", role => stat, optional => true},
        #{key => consumeOnUse, label => "Consume", role => stat, optional => true},
        #{key => trigger, label => "Trigger", role => stat, optional => true},
        #{key => fireRate, label => "Fire rate", role => stat, optional => true},
        #{key => magazineSize, label => "Mag", role => stat, optional => true},
        #{key => reloadTime, label => "Reload", role => stat, optional => true},
        #{key => accuracy, label => "Accuracy", role => stat, optional => true},
        #{key => noise, label => "Noise", role => stat, optional => true},
        #{key => damagePerShot, label => "Dmg/shot", role => stat, optional => true},
        #{key => abilitiesCount, label => "Abilities", role => stat, optional => true},
        #{key => abilities, label => "Ability names", role => details, optional => true},
        #{key => uniqueName, label => "Unique", role => id, optional => true}
    ].
