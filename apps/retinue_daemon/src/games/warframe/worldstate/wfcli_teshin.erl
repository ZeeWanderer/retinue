%%%-------------------------------------------------------------------
%% Calculated Steel Path offerings sold by Teshin.
%% Reference algorithm:
%% https://github.com/WFCD/warframe-worldstate-parser/blob/master/lib/models/SteelPathOffering.ts
%% Reference catalog:
%% https://github.com/WFCD/warframe-worldstate-data/blob/master/data/steelPath.json
%%%-------------------------------------------------------------------
-module(wfcli_teshin).

-include_lib("retinue_daemon/include/wfcli_worldstate.hrl").

-export([load/1, inventory/0, inventory/1, inventory_at/2,
         current_reward/1, rotation/0, evergreen/0]).

-define(ROTATION_START_SECONDS, 1605484800). % 2020-11-16T00:00:00Z
-define(WEEK_SECONDS, 604800).

-type offering() :: #{name := string(), cost := non_neg_integer()}.
-type entry() :: map().

-doc "Create the empty snapshot used to route calculated Teshin requests through retinue_daemon.".
-spec load(map()) -> {ok, #ws{}, calculated}.
load(_Opts) ->
    {ok, #ws{raw = #{}}, calculated}.

-doc "Return current weekly Steel Path reward followed by Teshin's evergreen stock.".
-spec inventory() -> [entry()].
inventory() ->
    inventory(#{}).

-spec inventory(map()) -> [entry()].
inventory(Opts) ->
    Now = now_seconds(Opts),
    inventory_at(Now, Opts).

-doc "Build inventory for a Unix timestamp; exposed for deterministic tests and future schedulers.".
-spec inventory_at(integer(), map()) -> [entry()].
inventory_at(Now, Opts) ->
    {Reward, Activation, Expiry} = current_reward(Now),
    Weekly = build_entry(weekly, 1, Reward, Activation, Expiry, Opts),
    Evergreens = [build_entry(evergreen, Index, Item, undefined, undefined, Opts)
                  || {Index, Item} <- lists:zip(lists:seq(1, length(evergreen())), evergreen())],
    [Weekly | Evergreens].

-doc "Calculate current reward and UTC week window using WFCD's 2020-11-16 anchor.".
-spec current_reward(integer()) -> {offering(), integer(), integer()}.
current_reward(Now) ->
    Week = floor_div(Now - ?ROTATION_START_SECONDS, ?WEEK_SECONDS),
    Index = positive_mod(Week, length(rotation())),
    Activation = ?ROTATION_START_SECONDS + Week * ?WEEK_SECONDS,
    Expiry = Activation + ?WEEK_SECONDS - 1,
    {lists:nth(Index + 1, rotation()), Activation, Expiry}.

-doc "Eight-week rotating offerings mirrored from WFCD warframe-worldstate-data.".
-spec rotation() -> [offering()].
rotation() ->
    [offering("Umbra Forma Blueprint", 150),
     offering("50,000 Kuva", 55),
     offering("Kitgun Riven Mod", 75),
     offering("3x Forma", 75),
     offering("Zaw Riven Mod", 75),
     offering("30,000 Endo", 150),
     offering("Rifle Riven Mod", 75),
     offering("Shotgun Riven Mod", 75)].

-doc "Always-available offerings mirrored from WFCD warframe-worldstate-data.".
-spec evergreen() -> [offering()].
evergreen() ->
    [offering("Veiled Riven Cipher", 20),
     offering("Bishamo Pauldrons Blueprint", 15),
     offering("Bishamo Cuirass Blueprint", 25),
     offering("Bishamo Helmet Blueprint", 20),
     offering("Bishamo Greaves Blueprint", 25),
     offering("10k Kuva", 15),
     offering("Primary Arcane Adapter", 15),
     offering("Secondary Arcane Adapter", 15),
     offering("Relic Pack", 15),
     offering("Stance Forma Blueprint", 10),
     offering("Trio Orbit Ephermera", 3),
     offering("Crania Ephemera", 85),
     offering("Counterbalance", 35),
     offering("Noggle Statue - Teshin", 35),
     offering("Gauss in Action Glyph", 15),
     offering("Grendel in Action Glyph", 15),
     offering("Protea in Action Glyph", 15),
     offering("Orokin Tea Set", 15),
     offering("Xaku in Action Glyph", 15)].

offering(Name, Cost) -> #{name => Name, cost => Cost}.

build_entry(Kind, Index, #{name := Name, cost := Cost}, Activation, Expiry, Opts) ->
    Availability = case Kind of weekly -> "Weekly"; evergreen -> "Evergreen" end,
    Data0 = #{<<"Name">> => list_to_binary(Name),
              <<"Cost">> => Cost,
              <<"Availability">> => list_to_binary(Availability)},
    Data = add_window(Data0, Activation, Expiry),
    Id = lists:flatten(io_lib:format("teshin-~s-~p", [atom_to_list(Kind), Index])),
    wfcli_entity_worldstate:build(teshin_item, Id, Name, Data, Opts).

add_window(Data, undefined, undefined) -> Data;
add_window(Data, Activation, Expiry) ->
    Data#{<<"Activation">> => Activation * 1000,
          <<"Expiry">> => Expiry * 1000}.

now_seconds(Opts) ->
    case maps:get(now_fun, Opts, undefined) of
        Fun when is_function(Fun, 0) -> Fun();
        _ -> erlang:system_time(second)
    end.

floor_div(Value, Divisor) when Value >= 0 -> Value div Divisor;
floor_div(Value, Divisor) -> -((-Value + Divisor - 1) div Divisor).

positive_mod(Value, Modulus) ->
    ((Value rem Modulus) + Modulus) rem Modulus.
