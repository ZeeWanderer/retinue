%%%-------------------------------------------------------------------
%% Entity builder for export-backed items and mods.
%%%-------------------------------------------------------------------
-module(wfcli_entity_exports).

-export([build_mod/2, build_item/2, mod_row_map/2, item_row_map/2,
         table_spec/1, columns_spec/0, column_spec/1, columns_for_kind/1,
         normalize_query_key/1, query_field/2, query_sort_field/2, query_match/5,
         polarity_symbol/1, polarity_display/1, default_sort/1]).

build_mod(Mod, Opts) ->
    Name = maps:get(name, Mod, ""),
    Id = entry_id(Mod, Name),
    Spec = #{row_map_fun => fun mod_row_map/2},
    wfcli_entity:build(mod, Id, Name, Mod, Opts, Spec).

build_item(Item, Opts) ->
    Name = maps:get(name, Item, ""),
    Id = entry_id(Item, Name),
    Spec = #{row_map_fun => fun item_row_map/2},
    wfcli_entity:build(item, Id, Name, Item, Opts, Spec).

entry_id(Map, Fallback) ->
    case maps:get(uniqueName, Map, "") of
        "" -> Fallback;
        Id -> Id
    end.

mod_row_map(Entry, _Opts) ->
    M = maps:get(data, Entry, #{}),
    #{
        name => maps:get(name, M, ""),
        type => maps:get(type, M, ""),
        polarity => maps:get(polarity, M, ""),
        rarity => maps:get(rarity, M, ""),
        baseDrain => maps:get(baseDrain, M, ""),
        fusionLimit => maps:get(fusionLimit, M, ""),
        compatName => maps:get(compatName, M, ""),
        description => retinue_text:join_list(maps:get(description, M, []), " "),
        effects => retinue_text:join_list(maps:get(effects, M, []), "; "),
        max_stats => retinue_text:join_list(maps:get(max_stats, M, []), " | "),
        uniqueName => maps:get(uniqueName, M, "")
    }.

item_row_map(Entry, _Opts) ->
    I = maps:get(data, Entry, #{}),
    #{
        name => maps:get(name, I, ""),
        file => maps:get(file, I, ""),
        productCategory => maps:get(productCategory, I, ""),
        masteryReq => maps:get(masteryReq, I, ""),
        totalDamage => maps:get(totalDamage, I, ""),
        criticalChance => maps:get(criticalChance, I, ""),
        criticalMultiplier => maps:get(criticalMultiplier, I, ""),
        procChance => maps:get(procChance, I, ""),
        armor => maps:get(armor, I, ""),
        health => maps:get(health, I, ""),
        shield => maps:get(shield, I, ""),
        sprintSpeed => maps:get(sprintSpeed, I, ""),
        buildTime => maps:get(buildTime, I, ""),
        buildPrice => maps:get(buildPrice, I, ""),
        skipBuildTimePrice => maps:get(skipBuildTimePrice, I, ""),
        resultType => maps:get(resultType, I, ""),
        ingredientsCount => maps:get(ingredientsCount, I, ""),
        secretIngredientsCount => maps:get(secretIngredientsCount, I, ""),
        resultCount => maps:get(resultCount, I, ""),
        consumeOnUse => maps:get(consumeOnUse, I, ""),
        trigger => maps:get(trigger, I, ""),
        fireRate => maps:get(fireRate, I, ""),
        magazineSize => maps:get(magazineSize, I, ""),
        reloadTime => maps:get(reloadTime, I, ""),
        accuracy => maps:get(accuracy, I, ""),
        noise => maps:get(noise, I, ""),
        damagePerShot => maps:get(damagePerShot, I, ""),
        abilitiesCount => maps:get(abilitiesCount, I, ""),
        abilities => retinue_text:join_list(maps:get(abilities, I, []), ", "),
        uniqueName => maps:get(uniqueName, I, "")
    }.

table_spec(Kind) ->
    #{columns => columns_for_kind(Kind), specs => columns_spec()}.

normalize_query_key(Key0) ->
    Key = string:lowercase(retinue_text:to_list(Key0)),
    case Key of
        "type" -> type;
        "polarity" -> polarity;
        "rarity" -> rarity;
        "compat" -> compat;
        "name" -> name;
        "text" -> text;
        "sort" -> sort;
        "file" -> file;
        "productcategory" -> productCategory;
        "masteryreq" -> masteryReq;
        "totaldamage" -> totalDamage;
        "criticalchance" -> criticalChance;
        "criticalmultiplier" -> criticalMultiplier;
        "procchance" -> procChance;
        "armor" -> armor;
        "health" -> health;
        "shield" -> shield;
        "sprintspeed" -> sprintSpeed;
        "buildtime" -> buildTime;
        "buildprice" -> buildPrice;
        "skipbuildtimeprice" -> skipBuildTimePrice;
        "resulttype" -> resultType;
        "ingredients" -> ingredientsCount;
        "secretingredients" -> secretIngredientsCount;
        "num" -> resultCount;
        "ingredientscount" -> ingredientsCount;
        "secretingredientscount" -> secretIngredientsCount;
        "resultcount" -> resultCount;
        "consumeonuse" -> consumeOnUse;
        "basedrain" -> baseDrain;
        "fusionlimit" -> fusionLimit;
        "trigger" -> trigger;
        "firerate" -> fireRate;
        "magazinesize" -> magazineSize;
        "reloadtime" -> reloadTime;
        "accuracy" -> accuracy;
        "noise" -> noise;
        "damagepershot" -> damagePerShot;
        "power" -> power;
        "stamina" -> stamina;
        "abilities" -> abilities;
        "abilitiescount" -> abilitiesCount;
        _ -> undefined
    end.

-doc "Resolve one export query key to its typed entity field; rejects fields absent from this kind.".
-spec query_field(mod | item, string() | atom()) -> {ok, map()} | error.
query_field(Kind, Key0) ->
    Key = normalize_query_key(Key0),
    case lists:member(Key, query_fields(Kind)) of
        true -> {ok, query_field_spec(Key)};
        false -> error
    end.

-doc "Resolve query aliases or displayed column labels to a sortable entity field.".
-spec query_sort_field(mod | item, string() | atom()) -> {ok, map()} | error.
query_sort_field(Kind, Key0) ->
    case query_field(Kind, Key0) of
        {ok, Spec} -> {ok, Spec};
        error -> sort_column_field(Kind, Key0)
    end.

-doc "Handle export-specific matching that cannot be represented as a plain scalar comparison.".
-spec query_match(mod, polarity, atom(), [string()], map()) -> boolean().
query_match(mod, polarity, Op, Values, Entry) ->
    Actual = maps:get(polarity, maps:get(data, Entry, #{}), ""),
    Tokens = lists:append([normalize_polarity_token(Value) || Value <- Values]),
    Found = polarity_match_any(Tokens, Actual),
    case Op of neq -> not Found; _ -> Found end.

query_fields(mod) ->
    [name, text, type, polarity, rarity, compat, baseDrain, fusionLimit];
query_fields(item) ->
    [name, text, file, productCategory, masteryReq, totalDamage, criticalChance,
     criticalMultiplier, procChance, armor, health, shield, sprintSpeed, buildTime,
     buildPrice, skipBuildTimePrice, resultType, ingredientsCount,
     secretIngredientsCount, resultCount, consumeOnUse, trigger, fireRate,
     magazineSize, reloadTime, accuracy, noise, damagePerShot, power, stamina,
     abilities, abilitiesCount].

query_field_spec(text) -> #{key => text, source => haystack, kind => string, default_op => contains};
query_field_spec(compat) -> #{key => compat, source => {row, compatName}, kind => string,
                              default_op => contains};
query_field_spec(abilities) -> #{key => abilities, source => {row, abilities}, kind => string,
                                 default_op => eq};
query_field_spec(polarity) -> #{key => polarity, source => {data, polarity}, kind => string,
                                default_op => eq, match => polarity};
query_field_spec(Key) ->
    #{key => Key, source => {data, Key}, kind => field_kind(Key), default_op => default_op(Key)}.

field_kind(Key) ->
    case lists:member(Key, [masteryReq, totalDamage, criticalChance, criticalMultiplier,
                            procChance, armor, health, shield, sprintSpeed, buildTime,
                            buildPrice, skipBuildTimePrice, ingredientsCount,
                            secretIngredientsCount, resultCount, fireRate, magazineSize,
                            reloadTime, baseDrain, fusionLimit, accuracy, damagePerShot,
                            power, stamina, abilitiesCount]) of
        true -> number;
        false -> string
    end.

default_op(Key) ->
    case lists:member(Key, [name, text, file, compat]) of
        true -> contains;
        false -> eq
    end.

sort_column_field(Kind, Key0) ->
    KeyText = string:lowercase(retinue_text:to_list(Key0)),
    case lists:dropwhile(
           fun(Spec) ->
               SpecKey = maps:get(key, Spec, undefined),
               Label = string:lowercase(retinue_text:to_list(maps:get(label, Spec, ""))),
               (SpecKey =:= undefined orelse string:lowercase(atom_to_list(SpecKey)) =/= KeyText)
                   andalso Label =/= KeyText
           end, columns_spec()) of
        [#{key := Key} = Column | _] ->
            case lists:member(Key, columns_for_kind(Kind)) of
                true -> {ok, #{key => Key, source => {row, Key},
                               kind => maps:get(kind, Column, field_kind(Key)), default_op => eq}};
                false -> error
            end;
        [] -> error
    end.

normalize_polarity_token(Value) ->
    Lower = string:lowercase(retinue_text:to_list(Value)),
    case Lower of
        "v" -> ["ap_attack", "v"];
        "d" -> ["ap_defense", "d"];
        "-" -> ["ap_tactic", "-"];
        "=" -> ["ap_power", "="];
        "y" -> ["ap_ward", "y"];
        "u" -> ["ap_umbra", "u"];
        "p" -> ["ap_precept", "p"];
        _ -> [Lower]
    end.

polarity_match_any(Filters, Value) ->
    Lower = string:lowercase(retinue_text:to_list(Value)),
    Symbol = string:lowercase(polarity_symbol(Value)),
    lists:any(fun(Filter) -> Filter =:= Lower orelse (Symbol =/= "" andalso Filter =:= Symbol) end,
              Filters).

-spec polarity_symbol(term()) -> string().
polarity_symbol(Polarity) -> wfcli_exports_schema:polarity_symbol(Polarity).

-spec polarity_display(term()) -> string().
polarity_display(Polarity) -> wfcli_exports_schema:polarity_display(Polarity).
default_sort(_Kind) ->
    [#{key => name, dir => asc}].

column_spec(Column) -> wfcli_exports_schema:column_spec(Column).

columns_for_kind(Kind) -> wfcli_exports_schema:columns_for_kind(Kind).

columns_spec() -> wfcli_exports_schema:columns_spec().
