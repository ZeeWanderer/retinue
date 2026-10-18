%%%-------------------------------------------------------------------
%% JSON-lines protocol shared by retinue_daemon and native companion clients.
%%%-------------------------------------------------------------------
-module(retinue_local_protocol).

-export([decode/1, encode/1, envelope_version/0, interfaces/0, interfaces/1,
         features/0, features/1, contract/0, contract/1, negotiate/1]).

-define(ENVELOPE_VERSION, 2).
-define(INTERFACE_SESSION, 2).
-define(INTERFACE_TRADE, 2).
-define(INTERFACE_DATASETS, 1).
-define(INTERFACE_PLAYER, 2).
-define(INTERFACE_GAME_METADATA, 2).
-define(INTERFACE_WORLDSTATE, 1).
-define(INTERFACE_NOTIFICATIONS, 1).
-define(INTERFACE_MARKET, 2).
-define(INTERFACE_OVERFRAME, 1).
-define(INTERFACE_RELICS, 1).
-define(INTERFACE_ASSETS, 1).
-define(INTERFACE_BUILDS, 1).
-define(INTERFACE_BUILD_STATS, 1).
-define(INTERFACE_DIAGNOSTICS, 1).
-define(INTERFACE_CHECKS, 1).

-doc "Version of the JSON-lines handshake and framing envelope.".
-spec envelope_version() -> pos_integer().
envelope_version() -> ?ENVELOPE_VERSION.

-doc "Current Unix-socket domain interface versions.".
-spec interfaces() -> #{binary() := pos_integer()}.
interfaces() ->
    #{<<"datasets">> => ?INTERFACE_DATASETS,
      <<"player">> => ?INTERFACE_PLAYER,
      <<"game_metadata">> => ?INTERFACE_GAME_METADATA,
      <<"worldstate">> => ?INTERFACE_WORLDSTATE,
      <<"notifications">> => ?INTERFACE_NOTIFICATIONS,
      <<"market">> => ?INTERFACE_MARKET,
      <<"overframe">> => ?INTERFACE_OVERFRAME,
      <<"relics">> => ?INTERFACE_RELICS,
      <<"assets">> => ?INTERFACE_ASSETS,
      <<"builds">> => ?INTERFACE_BUILDS,
      <<"build_stats">> => ?INTERFACE_BUILD_STATS,
      <<"diagnostics">> => ?INTERFACE_DIAGNOSTICS,
      <<"checks">> => ?INTERFACE_CHECKS}.

-doc "Optional Unix-socket features offered by retinue_daemon.".
-spec features() -> [binary()].
features() -> [<<"companion.command">>, <<"companion.diagnostics">>, <<"diagnostics.report">>].

interfaces(<<"warframe">>) -> interfaces();
interfaces(<<"poe2">>) -> #{<<"session">> => ?INTERFACE_SESSION, <<"trade">> => ?INTERFACE_TRADE,
                           <<"checks">> => ?INTERFACE_CHECKS};
interfaces(_) -> #{}.

features(<<"warframe">>) -> features();
features(<<"poe2">>) -> [<<"trade.prediction">>];
features(_) -> [].

-doc "Return current Unix-socket handshake contract.".
-spec contract() -> map().
contract() ->
    contract(<<"warframe">>).

contract(Game) ->
    #{<<"envelope">> => envelope_version(),
      <<"game">> => Game,
      <<"interfaces">> => interfaces(Game),
      <<"features">> => features(Game)}.

-doc "Negotiate one client's required interfaces and optional features.".
-spec negotiate(map()) -> map().
negotiate(Client) when is_map(Client) ->
    Game = maps:get(<<"game">>, Client, undefined),
    Server = contract(Game),
    GameMismatches = case lists:member(Game, [<<"warframe">>, <<"poe2">>]) of
        true -> [];
        false -> [#{<<"kind">> => <<"game">>, <<"required">> => nullable(Game),
                    <<"available">> => [<<"warframe">>, <<"poe2">>]}]
    end,
    Envelope = envelope_version(),
    EnvelopeMismatches = case maps:get(<<"envelope">>, Client, undefined) of
        Envelope -> [];
        Required -> [#{<<"kind">> => <<"envelope">>,
                       <<"required">> => nullable(Required),
                       <<"available">> => Envelope}]
    end,
    InterfaceMismatches = interface_mismatches(
                            maps:get(<<"interfaces">>, Client, undefined),
                            maps:get(<<"interfaces">>, Server)),
    Mismatches = GameMismatches ++ EnvelopeMismatches ++ InterfaceMismatches,
    RequestedFeatures = case maps:get(<<"features">>, Client, []) of
        Values when is_list(Values) -> Values;
        _Invalid -> []
    end,
    Negotiated = [Feature || Feature <- features(Game),
                             lists:member(Feature, RequestedFeatures)],
    Server#{<<"compatible">> => Mismatches =:= [],
            <<"mismatches">> => Mismatches,
            <<"features">> => Negotiated};
negotiate(_Client) ->
    (contract())#{<<"compatible">> => false,
                  <<"mismatches">> =>
                      [#{<<"kind">> => <<"envelope">>,
                         <<"required">> => null,
                         <<"available">> => envelope_version()}]}.

-doc "Decode one JSON protocol line into a binary-keyed map.".
-spec decode(binary()) -> {ok, map()} | {error, term()}.
decode(Line) when is_binary(Line) ->
    try jsone:decode(Line, [{object_format, map}]) of
        Map when is_map(Map) -> {ok, Map};
        Other -> {error, {expected_object, Other}}
    catch
        error:Reason -> {error, {invalid_json, Reason}}
    end.

-doc "Encode one binary-keyed protocol map as a JSON line.".
-spec encode(map()) -> iodata().
encode(Map) when is_map(Map) ->
    [jsone:encode(Map), $\n].

interface_mismatches(Required, Available) when is_map(Required),
                                                is_map(Available) ->
    [#{<<"kind">> => <<"interface">>, <<"interface">> => Name,
       <<"required">> => nullable(Version),
       <<"available">> => nullable(maps:get(Name, Available, undefined))}
     || {Name, Version} <- lists:sort(maps:to_list(Required)),
        not is_integer(Version) orelse Version =< 0 orelse
        maps:get(Name, Available, undefined) =/= Version];
interface_mismatches(Required, _Available) when is_map(Required) ->
    [#{<<"kind">> => <<"interfaces">>, <<"required">> => <<"valid">>,
       <<"available">> => <<"invalid">>}];
interface_mismatches(_Required, _Available) ->
    [#{<<"kind">> => <<"interfaces">>, <<"required">> => null}].

nullable(undefined) -> null;
nullable(Value) -> Value.
