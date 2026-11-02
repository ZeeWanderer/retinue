-module(wf_item_catalog_eunit).

-include_lib("eunit/include/eunit.hrl").

recipe_result_aliases_player_blueprints_test_() ->
    {setup, fun setup/0, fun cleanup/1,
     fun(_Root) -> fun resolves_recipe_alias/0 end}.

drop_locations_resolve_exact_refinements_test_() ->
    {setup, fun setup/0, fun cleanup/1, fun(#{root := Root}) -> fun() ->
        Path = filename:join(Root, "WFCDItems.json"),
        {ok, Body} = file:read_file(Path),
        #{<<"entries">> := [Item]} = Wrapper = jsone:decode(Body),
        [Part | Rest] = maps:get(<<"components">>, Item),
        Drops = [#{<<"location">> => Location, <<"chance">> => Chance}
                 || {Location, Chance} <- [{<<"Axi I3 Relic">>, 11},
                     {<<"Axi I3 Relic (Radiant)">>, 20},
                     {<<"Unknown Relic">>, 2}]],
        Relics = [#{<<"category">> => <<"Relics">>,
                     <<"name">> => <<"Axi I3 ", Refinement/binary>>,
                     <<"uniqueName">> => Id}
                  || {Refinement, Id} <- [{<<"Intact">>, <<"testBronze">>},
                                         {<<"Radiant">>, <<"testPlatinum">>}]],
        ok = file:write_file(Path, jsone:encode(Wrapper#{<<"entries">> =>
            [Item#{<<"components">> => [Part#{<<"drops">> => Drops} | Rest]} | Relics]})),
        {ok, [Resolved | _], _} = wf_item_catalog:load(),
        [ResolvedPart | _] = maps:get(<<"components">>, Resolved),
        [Intact, Radiant, Unknown] = maps:get(<<"drops">>, ResolvedPart),
        ?assertEqual(<<"testBronze">>, maps:get(<<"uniqueName">>, Intact)),
        ?assertEqual(<<"testPlatinum">>, maps:get(<<"uniqueName">>, Radiant)),
        ?assertEqual(20, maps:get(<<"chance">>, Radiant)),
        ?assertNot(maps:is_key(<<"uniqueName">>, Unknown))
    end end}.

component_names_follow_official_resource_updates_test_() ->
    {setup, fun setup/0, fun cleanup/1,
     fun(#{root := Root, result := Result}) ->
         fun() ->
             {ok, Initial, InitialMeta} = wf_item_catalog:load(),
             [InitialItem] = Initial,
             [_, InitialPart] = maps:get(<<"components">>, InitialItem),
             ?assertNot(maps:is_key(<<"fullName">>, InitialPart)),
             ?assertEqual(undefined, maps:get(name_version, InitialMeta)),
             Path = filename:join(Root, "ExportResources_en.json"),
             SaveName = fun(Name) ->
                 file:write_file(Path, jsone:encode(#{<<"ExportResources">> => [
                     #{<<"uniqueName">> => Result, <<"name">> => Name}]}))
             end,
             ok = SaveName(<<"Other Weapon Receiver">>),
             {ok, [Item], Meta} = wf_item_catalog:load(),
             [_, Part] = maps:get(<<"components">>, Item),
             ?assertEqual(<<"Receiver">>, maps:get(<<"name">>, Part)),
             ?assertEqual(<<"Other Weapon Receiver">>, maps:get(<<"fullName">>, Part)),
             ?assertMatch({_, _}, maps:get(name_version, Meta)),
             ok = SaveName(<<"Updated Weapon Receiver">>),
             {ok, [Updated], UpdatedMeta} = wf_item_catalog:load(),
             [_, UpdatedPart] = maps:get(<<"components">>, Updated),
             ?assertEqual(<<"Updated Weapon Receiver">>,
                          maps:get(<<"fullName">>, UpdatedPart)),
             ?assertNotEqual(maps:get(name_version, Meta), maps:get(name_version, UpdatedMeta))
         end
     end}.

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
