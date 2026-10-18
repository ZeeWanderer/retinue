-module(retinue_poe2_trade_http).
-export([request/4, data/1, predict/1, headers/1, freshness/2, validators/1,
         cooldown/3, spend/3, observe/5, retry_after/2, automatic_allowed/2]).

-define(TIMEOUT, 8000).
-define(MAX_BODY, 8388608).

%% Dedicated connections have no cookie store, credentials, redirects or retries.
request(Method, Path, Conditional, Body) ->
    request("www.pathofexile.com", ?MAX_BODY, Method, Path, Conditional, Body).

data(Path) ->
    request("repoe-fork.github.io", 32 * 1024 * 1024, get, Path, [], <<>>).

predict(Body) ->
    request("prediction.poeoverlay.com", 65536, post, <<"/api/item/price">>, [], Body, 15000).

request(Host, MaxBody, Method, Path, Conditional, Body) ->
    request(Host, MaxBody, Method, Path, Conditional, Body, ?TIMEOUT).
request(Host, MaxBody, Method, Path, Conditional, Body, Timeout) ->
    try
        {ok, _} = application:ensure_all_started(gun),
        Deadline = erlang:monotonic_time(millisecond) + Timeout,
        TLS = [{verify, verify_peer}, {cacerts, public_key:cacerts_get()},
               {server_name_indication, Host},
               {customize_hostname_check, [{match_fun, public_key:pkix_verify_hostname_match_fun(https)}]}],
        {ok, Conn} = gun:open(Host, 443,
                             #{transport => tls, tls_opts => TLS, protocols => [http],
                               retry => 0, connect_timeout => 3000, tls_handshake_timeout => 3000}),
        try
            case gun:await_up(Conn, remaining(Deadline)) of
                {ok, _} ->
                    Headers = [{<<"accept">>, <<"application/json">>},
                               {<<"accept-encoding">>, <<"identity">>},
                               {<<"user-agent">>, <<"Retinue/PoE2-Trade">>},
                               {<<"content-type">>, <<"application/json">>} | Conditional],
                    Ref = gun:request(Conn, string:uppercase(atom_to_binary(Method)), Path, Headers, Body, #{flow => 1}),
                    response(Conn, Ref, Deadline, MaxBody);
                _ -> {error, upstream_unavailable}
            end
        after gun:close(Conn) end
    catch _:_ -> {error, upstream_unavailable} end.

remaining(Deadline) -> max(0, Deadline - erlang:monotonic_time(millisecond)).

response(Conn, Ref, Deadline, MaxBody) ->
    case gun:await(Conn, Ref, remaining(Deadline)) of
        {response, Fin, Status, RawHeaders} ->
            Headers = headers(RawHeaders),
            case integer(maps:get(<<"content-length">>, Headers, <<"0">>), 0) > MaxBody of
                true -> {error, response_too_large};
                false when Fin =:= fin -> {ok, Status, Headers, <<>>};
                false -> body(Conn, Ref, Deadline, Status, Headers, [], 0, MaxBody)
            end;
        {inform, _, _} -> response(Conn, Ref, Deadline, MaxBody);
        _ -> {error, upstream_timeout}
    end.

body(Conn, Ref, Deadline, Status, Headers, Parts, Size, MaxBody) ->
    case gun:await(Conn, Ref, remaining(Deadline)) of
        {data, _, Data} when byte_size(Data) + Size > MaxBody -> {error, response_too_large};
        {data, fin, Data} -> {ok, Status, Headers, iolist_to_binary(lists:reverse([Data | Parts]))};
        {data, nofin, Data} ->
            gun:update_flow(Conn, Ref, 1),
            body(Conn, Ref, Deadline, Status, Headers, [Data | Parts], Size + byte_size(Data), MaxBody);
        {trailers, _} -> {ok, Status, Headers, iolist_to_binary(lists:reverse(Parts))};
        _ -> {error, upstream_timeout}
    end.

%% Never retain response cookies or unrelated headers in cache, state or errors.
headers(Map) when is_map(Map) -> headers(maps:to_list(Map));
headers(Pairs) ->
    maps:from_list([{K, iolist_to_binary(V)} || {Name, V} <- Pairs,
        K <- [string:lowercase(iolist_to_binary(Name))],
        lists:member(K, [<<"cache-control">>, <<"etag">>, <<"last-modified">>, <<"age">>,
                         <<"date">>, <<"expires">>, <<"retry-after">>, <<"content-length">>]) orelse
        string:prefix(K, <<"x-rate-limit-">>) =/= nomatch]).

validators(Headers) ->
    [{Target, V} || {Source, Target} <- [{<<"etag">>, <<"if-none-match">>},
                                       {<<"last-modified">>, <<"if-modified-since">>}],
                   {ok, V} <- [maps:find(Source, Headers)]].

freshness(Headers, Now) ->
    Control = string:lowercase(maps:get(<<"cache-control">>, Headers, <<>>)),
    NoStore = binary:match(Control, <<"no-store">>) =/= nomatch,
    NoCache = binary:match(Control, <<"no-cache">>) =/= nomatch,
    Age = max(integer(maps:get(<<"age">>, Headers, <<"0">>), 0),
              max(0, Now - date(maps:get(<<"date">>, Headers, <<>>), Now))),
    TTL = case re:run(Control, <<"(?:^|,)\\s*max-age=\"?([0-9]+)">>, [{capture, [1], binary}]) of
        {match, [Seconds]} -> integer(Seconds, 0) - Age;
        nomatch -> date(maps:get(<<"expires">>, Headers, <<>>), Now + 300) - Now
    end,
    #{store => not NoStore, expires => Now + case NoStore orelse NoCache of
        true -> 0; false -> max(0, min(TTL, 86400)) end}.

cooldown(Group, Limits, Now) ->
    Bucket = maps:get(bucket(Group, Limits), Limits, #{}),
    Windows = maps:get(windows, Bucket, []),
    Until = lists:foldl(fun(#{count := Count, limit := Limit, expires := Expiry,
                             blocked := Blocked}, Acc) ->
        max(Acc, max(Blocked, case Count >= Limit of true -> Expiry; false -> Now end))
    end, max(maps:get(global, Limits, Now), maps:get(next, Bucket, Now)), Windows),
    max(0, Until - Now).

automatic_allowed(Limits, Now) ->
    lists:all(fun(Group) ->
        Bucket = maps:get(bucket(Group, Limits), Limits, #{}),
        cooldown(Group, Limits, Now) =:= 0 andalso lists:all(fun(W) ->
            maps:get(expires, W) =< Now orelse maps:get(count, W) / maps:get(limit, W) < 0.75
        end, maps:get(windows, Bucket, []))
    end, [search, fetch]).

spend(Group, Limits, Now) ->
    Key = bucket(Group, Limits),
    Bucket = maps:get(Key, Limits, #{}),
    Windows = [case maps:get(expires, W) =< Now of
                   true -> W#{count => 1, expires => Now + maps:get(period, W)};
                   false -> W#{count => maps:get(count, W) + 1}
               end || W <- maps:get(windows, Bucket, [])],
    Limits#{Key => Bucket#{windows => Windows}}.

observe(Group, Status, Headers, Limits, Now) ->
    Policy = maps:get(<<"x-rate-limit-policy">>, Headers, atom_to_binary(Group)),
    Key = {policy, Policy},
    Old = maps:get(Key, Limits, maps:get(bucket(Group, Limits), Limits, #{})),
    Names = binary:split(string:lowercase(maps:get(<<"x-rate-limit-rules">>, Headers, <<>>)),
                         <<",">>, [global]),
    Parsed = lists:append([windows(string:trim(Name), Headers, Now) || Name <- Names]),
    Windows = case Parsed of [] -> maps:get(windows, Old, []); _ -> Parsed end,
    Retry = case retry_after(maps:get(<<"retry-after">>, Headers, <<>>), Now) of
        0 when Status =:= 429 -> 60;
        N -> N
    end,
    Limits#{Group => Key, Key => Old#{windows => Windows},
            global => max(maps:get(global, Limits, Now), Now + Retry)}.

bucket(Group, Limits) -> maps:get(Group, Limits, {policy, atom_to_binary(Group)}).

windows(<<>>, _, _) -> [];
windows(Name, Headers, Now) ->
    Rules = triples(maps:get(<<"x-rate-limit-", Name/binary>>, Headers, <<>>)),
    States = triples(maps:get(<<"x-rate-limit-", Name/binary, "-state">>, Headers, <<>>)),
    [#{limit => Limit, period => Period, count => Count, expires => Now + Period,
       blocked => Now + Blocked}
     || {Limit, Period, _Penalty} <- Rules, Limit > 0, Period > 0,
        {Count, StatePeriod, Blocked} <- States, StatePeriod =:= Period].

triples(Text) ->
    lists:filtermap(fun(Part) ->
        case [integer(string:trim(N), -1) || N <- binary:split(Part, <<":">>, [global])] of
            [A, B, C] when A >= 0, B >= 0, C >= 0 -> {true, {A, B, C}};
            _ -> false
        end
    end, binary:split(Text, <<",">>, [global])).

retry_after(Text, Now) ->
    case integer(string:trim(Text), -1) of
        N when N >= 0 -> N;
        _ -> max(0, date(Text, Now) - Now)
    end.

integer(Bin, Default) -> try binary_to_integer(Bin) catch _:_ -> Default end.
date(Text, Default) ->
    try calendar:datetime_to_gregorian_seconds(httpd_util:convert_request_date(binary_to_list(Text)))
        - 62167219200
    catch _:_ -> Default end.
