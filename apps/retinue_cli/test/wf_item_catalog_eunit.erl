-module(wf_item_catalog_eunit).

-include_lib("eunit/include/eunit.hrl").

recipe_result_aliases_player_blueprints_test_() ->
    {setup, fun setup/0, fun cleanup/1,
     fun(_Root) -> fun resolves_recipe_alias/0 end}.

setup() ->
    Root = filename:join(
             "/tmp", "wfcli-item-catalog-" ++
                 integer_to_list(erlang:unique_integer([positive]))),
    CatalogPath = filename:join(Root, "WFCDItems.json"),
    RecipePath = filename:join(Root, "ExportRecipes_en.json"),
    Result = <<"/Lotus/Types/Recipes/Weapons/WeaponParts/InternalReceiver">>,
    Alias = <<"/Lotus/Types/Recipes/Weapons/WeaponParts/PublicReceiverBlueprint">>,
    Blueprint = <<"/Lotus/Types/Recipes/Weapons/TestWeaponBlueprint">>,
    Weapon = <<"/Lotus/Weapons/TestWeapon">>,
    Catalog = #{<<"version">> => <<"test">>, <<"fetchedAt">> => 1,
                <<"entries">> => [
                    #{<<"uniqueName">> => Weapon,
                      <<"name">> => <<"Test Weapon">>,
                      <<"category">> => <<"Primary">>,
                      <<"components">> => [
                          #{<<"uniqueName">> => Blueprint,
                            <<"name">> => <<"Blueprint">>,
                            <<"imageName">> => <<"blueprint.png">>},
                          #{<<"uniqueName">> => Result,
                            <<"name">> => <<"Receiver">>}
                      ]}
                ]},
    Recipes = #{<<"ExportRecipes">> => [
        #{<<"uniqueName">> => Alias, <<"resultType">> => Result},
        #{<<"uniqueName">> => Blueprint, <<"resultType">> => Weapon}
    ]},
    ok = filelib:ensure_dir(filename:join(Root, "placeholder")),
    ok = file:write_file(CatalogPath, jsone:encode(Catalog)),
    ok = file:write_file(RecipePath, jsone:encode(Recipes)),
    application:set_env(retinue_daemon, item_catalog_file, CatalogPath),
    application:set_env(retinue_daemon, item_recipe_file, RecipePath),
    #{root => Root, alias => Alias, result => Result}.

cleanup(#{root := Root}) ->
    application:unset_env(retinue_daemon, item_catalog_file),
    application:unset_env(retinue_daemon, item_recipe_file),
    _ = file:del_dir_r(Root),
    ok.

resolves_recipe_alias() ->
    {ok, Catalog, Meta} = wf_item_catalog:load(),
    Index = wf_item_catalog:index(Catalog),
    Alias = <<"/Lotus/Types/Recipes/Weapons/WeaponParts/PublicReceiverBlueprint">>,
    Result = <<"/Lotus/Types/Recipes/Weapons/WeaponParts/InternalReceiver">>,
    Entry = wf_item_catalog:lookup(Alias, Index),
    ?assertEqual(Result, maps:get(<<"recipeResultType">>, Entry)),
    ?assertEqual(<<"Test Weapon">>, maps:get(<<"parentName">>, Entry)),
    Blueprint = <<"/Lotus/Types/Recipes/Weapons/TestWeaponBlueprint">>,
    BlueprintEntry = wf_item_catalog:lookup(Blueprint, Index),
    ?assertEqual(true, maps:get(<<"component">>, BlueprintEntry)),
    ?assertEqual(<<"Blueprint">>, maps:get(<<"name">>, BlueprintEntry)),
    ?assertEqual(<<"blueprint.png">>, maps:get(<<"imageName">>, BlueprintEntry)),
    ?assertMatch({_Modified, _Size}, maps:get(recipe_version, Meta)).
