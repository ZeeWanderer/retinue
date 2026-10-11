%%%-------------------------------------------------------------------
%% Data-only client/daemon protocol contract.
%%%-------------------------------------------------------------------
-module(retinue_protocol).

-export([handshake_version/0, interfaces/0, features/0, contract/0,
         negotiate/1, compatibility/2]).

-doc "Version of the Erlang distribution handshake envelope.".
-spec handshake_version() -> pos_integer().
handshake_version() -> 1.

-doc "Current Erlang RPC interface versions.".
-spec interfaces() -> #{atom() := pos_integer()}.
interfaces() ->
    #{daemon => 2,
      poe2_session => 1,
      worldstate => 2,
      query => 2,
      metadata => 1,
      forma => 1,
      build_stats => 1,
      player => 1,
      market => 1,
      notifications => 1,
      diagnostics => 1,
      companion => 2}.

-doc "Optional Erlang RPC features offered by this build.".
-spec features() -> [atom()].
features() -> [].

-doc "Return this build's Erlang RPC handshake contract.".
-spec contract() -> map().
contract() ->
    #{handshake => handshake_version(),
      interfaces => interfaces(),
      features => features()}.

-doc "Compare a client's required RPC contract with the current daemon contract.".
-spec negotiate(map()) -> map().
negotiate(Client) when is_map(Client) ->
    Server = contract(),
    Mismatches = contract_mismatches(Client, Server),
    RequestedFeatures = case maps:get(features, Client, []) of
        Values when is_list(Values) -> Values;
        _Invalid -> []
    end,
    NegotiatedFeatures = [Feature || Feature <- features(),
                                     lists:member(Feature, RequestedFeatures)],
    Server#{compatible => Mismatches =:= [],
            mismatches => Mismatches,
            features => NegotiatedFeatures};
negotiate(_Client) ->
    (contract())#{compatible => false,
                  mismatches => [#{kind => handshake,
                                   required => invalid,
                                   available => handshake_version()}]}.

-doc "Validate one required RPC contract against an offered contract.".
-spec compatibility(map(), map()) -> ok | {error, [map()]}.
compatibility(Required, Offered) when is_map(Required), is_map(Offered) ->
    case contract_mismatches(Required, Offered) of
        [] -> ok;
        Mismatches -> {error, Mismatches}
    end;
compatibility(_Required, _Offered) ->
    {error, [#{kind => handshake, required => invalid, available => invalid}]}.

contract_mismatches(Required, Available) ->
    RequiredHandshake = maps:get(handshake, Required, undefined),
    AvailableHandshake = maps:get(handshake, Available, undefined),
    EnvelopeMismatch = case RequiredHandshake =:= AvailableHandshake andalso
                            is_integer(RequiredHandshake) andalso
                            RequiredHandshake > 0 of
        true -> [];
        false -> [#{kind => handshake,
                    required => RequiredHandshake,
                    available => AvailableHandshake}]
    end,
    EnvelopeMismatch ++
        interface_mismatches(maps:get(interfaces, Required, undefined),
                             maps:get(interfaces, Available, #{})).

interface_mismatches(Required, Available) when is_map(Required),
                                                is_map(Available) ->
    [#{kind => interface, interface => Name,
       required => Version, available => maps:get(Name, Available, undefined)}
     || {Name, Version} <- lists:sort(maps:to_list(Required)),
        not is_integer(Version) orelse Version =< 0 orelse
        maps:get(Name, Available, undefined) =/= Version];
interface_mismatches(Required, _Available) when is_map(Required) ->
    [#{kind => interfaces, required => valid, available => invalid}];
interface_mismatches(_Required, _Available) ->
    [#{kind => interfaces, required => invalid}].
