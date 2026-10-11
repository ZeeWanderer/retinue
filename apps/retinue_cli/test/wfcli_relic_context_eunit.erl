%%%-------------------------------------------------------------------
%% EUnit coverage for reward, set, asset, and player ownership joins.
%%%-------------------------------------------------------------------
-module(wfcli_relic_context_eunit).

-include_lib("eunit/include/eunit.hrl").

joins_real_market_shape_to_player_inventory_test() ->
    Result = build(player_snapshot([root_entry()], [], []), catalog(false)),
    [Context] = maps:get(<<"items">>, Result),
    ?assertEqual(2, maps:get(<<"count_owned">>, Context)),
    ?assertEqual(2, maps:get(<<"total_to_own">>, Context)),
    ?assertEqual(100, maps:get(<<"ducats">>, Context)),
    ?assertEqual(true, maps:get(<<"crafted">>, Context)),
    ?assertEqual(false, maps:get(<<"set_complete">>, Context)),
    ?assertEqual(false, maps:get(<<"vaulted">>, Context)),
    ?assertEqual(18, maps:get(<<"lowest_sell">>, Context)),
    ?assertEqual(75, maps:get(<<"set_price">>, Context)),
    Parts = maps:get(<<"parts">>, Context),
    ?assertEqual(3, length(Parts)),
    Root = hd([Part || Part <- Parts, maps:get(<<"set_root">>, Part)]),
    ?assertEqual(<<"saryn_prime_set">>, maps:get(<<"slug">>, Root)),
    Systems = hd([Part || Part <- Parts,
                          maps:get(<<"slug">>, Part) =:=
                          <<"saryn_prime_systems_blueprint">>]),
    ?assertEqual(2, maps:get(<<"required">>, Systems)),
    ?assertEqual(#{<<"platinum">> => 124, <<"ducats">> => 915},
                 maps:get(<<"account">>, Result)),
    Asset = maps:get(<<"asset">>, Context),
    ?assertEqual(<<"market:saryn_prime_chassis_blueprint">>,
                 maps:get(<<"id">>, Asset)),
    ?assertEqual(<<"sub_icons/warframe/prime_chassis_128x128.png">>,
                 maps:get(<<"image_name">>, Asset)),
    [Vaulted] = maps:get(<<"items">>,
                         build(player_snapshot([], [], []), catalog(true))),
    ?assertEqual(true, maps:get(<<"vaulted">>, Vaulted)).

crafted_status_matches_aleca_history_test() ->
    Mastered = player_snapshot([], [mastery_entry(900000)], []),
    Pending = player_snapshot([], [], [pending_entry()]),
    Partial = player_snapshot([], [mastery_entry(899999)], []),
    Empty = player_snapshot([], [], []),
    ?assertEqual(true, crafted(Mastered)),
    ?assertEqual(true, crafted(Pending)),
    ?assertEqual(false, crafted(Partial)),
    ?assertEqual(false, crafted(Empty)).

crafted(Player) ->
    [Context] = maps:get(<<"items">>, build(Player, catalog(false))),
    maps:get(<<"crafted">>, Context).

build(Player, Catalog) ->
    wfcli_relic_context:build(
      [<<"saryn_prime_chassis_blueprint">>], items(), details(), quotes(),
      Player, relics(), Catalog).

items() ->
    [item(<<"set">>, <<"saryn_prime_set">>, <<"Saryn Prime Set">>,
          <<"/Lotus/Powersuits/Saryn/SarynPrime">>, [<<"warframe">>, <<"prime">>, <<"set">>]),
     item(<<"chassis">>, <<"saryn_prime_chassis_blueprint">>,
          <<"Saryn Prime Chassis Blueprint">>,
          <<"/Lotus/Types/Recipes/WarframeRecipes/SarynPrimeChassisBlueprint">>,
          [<<"component">>, <<"warframe">>, <<"prime">>, <<"blueprint">>]),
     item(<<"systems">>, <<"saryn_prime_systems_blueprint">>,
          <<"Saryn Prime Systems Blueprint">>,
          <<"/Lotus/Types/Recipes/WarframeRecipes/SarynPrimeSystemsBlueprint">>,
          [<<"component">>, <<"warframe">>, <<"prime">>, <<"blueprint">>])].

details() ->
    #{<<"saryn_prime_chassis_blueprint">> => #{data =>
        (lists:nth(2, items()))#{
            <<"ducats">> => 100,
            <<"quantityInSet">> => 2,
            <<"setRoot">> => false,
            <<"setParts">> => [<<"set">>, <<"chassis">>, <<"systems">>],
            <<"i18n">> => #{<<"en">> => #{
                <<"name">> => <<"Saryn Prime Chassis Blueprint">>,
                <<"subIcon">> =>
                    <<"sub_icons/warframe/prime_chassis_128x128.png">>
            }}
        }}}.

quotes() ->
    #{<<"saryn_prime_chassis_blueprint">> =>
          #{lowest_sell => 18, highest_buy => 15},
      <<"saryn_prime_set">> => #{lowest_sell => 75}}.

relics() ->
    #{<<"active">> => #{vaulted => false,
                          rewards => [#{slug =>
                              <<"saryn_prime_chassis_blueprint">>}]}}.

catalog(Vaulted) ->
    [#{<<"uniqueName">> => <<"/Lotus/Powersuits/Saryn/SarynPrime">>,
       <<"name">> => <<"Saryn Prime">>,
       <<"category">> => <<"Warframes">>,
       <<"type">> => <<"Warframe">>,
       <<"vaulted">> => Vaulted,
       <<"components">> => [
           #{<<"uniqueName">> =>
                 <<"/Lotus/Types/Recipes/WarframeRecipes/SarynPrimeChassisComponent">>,
             <<"itemCount">> => 1},
           #{<<"uniqueName">> =>
                 <<"/Lotus/Types/Recipes/WarframeRecipes/SarynPrimeSystemsComponent">>,
             <<"itemCount">> => 1},
           #{<<"uniqueName">> =>
                 <<"/Lotus/Types/Recipes/WarframeRecipes/SarynPrimeSystemsComponent">>,
             <<"itemCount">> => 1}
       ]}].

item(Id, Slug, Name, GameRef, Tags) ->
    #{<<"id">> => Id, <<"slug">> => Slug, <<"gameRef">> => GameRef,
      <<"tags">> => Tags,
      <<"i18n">> => #{<<"en">> => #{
          <<"name">> => Name,
          <<"thumb">> => <<"items/images/en/thumbs/", Slug/binary, ".png">>
      }}}.

player_snapshot(Equipment, Mastery, Pending) ->
    #{data => #{<<"inventory">> => #{
        <<"profile">> => #{<<"premium_credits">> => 124},
        <<"index">> => #{
            <<"equipment">> => Equipment,
            <<"mastery">> => Mastery,
            <<"pending_recipes">> => Pending,
            <<"stacks">> => [
                #{<<"item_type">> =>
                      <<"/Lotus/Types/Recipes/WarframeRecipes/SarynPrimeChassisBlueprint">>,
                  <<"count">> => 2},
                #{<<"item_type">> =>
                      <<"/Lotus/Types/Recipes/WarframeRecipes/SarynPrimeSystemsBlueprint">>,
                  <<"count">> => 1},
                #{<<"item_type">> => <<"/Lotus/Types/Items/MiscItems/PrimeBucks">>,
                  <<"count">> => 915}
            ]}
        }
    }}.

root_entry() ->
    #{<<"item_type">> => <<"/Lotus/Powersuits/Saryn/SarynPrime">>,
      <<"count">> => 1}.

mastery_entry(Xp) ->
    #{<<"item_type">> => <<"/Lotus/Powersuits/Saryn/SarynPrime">>,
      <<"xp">> => Xp}.

pending_entry() ->
    #{<<"item_type">> =>
          <<"/Lotus/Types/Recipes/WarframeRecipes/SarynPrimeSystemsBlueprint">>}.
