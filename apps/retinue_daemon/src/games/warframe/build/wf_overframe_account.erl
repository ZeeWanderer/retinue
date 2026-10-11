%%%-------------------------------------------------------------------
%% Overframe browser-session persistence and validation.
%%%-------------------------------------------------------------------
-module(wf_overframe_account).

-export([snapshot/0, store_session/1, logout/0, path/0, session_headers/0]).

-define(AUTH_URL, "https://overframe.gg/api/v1/auth-info/").

-doc "Validate the persisted Overframe browser session.".
-spec snapshot() -> {ok, map()} | {error, binary()}.
snapshot() ->
    case load(path()) of
        {ok, undefined} -> {ok, empty_snapshot(false)};
        {ok, Cookies} -> validate(Cookies);
        {error, Reason} -> {error, format_error("read", Reason)}
    end.

-doc "Persist browser cookies from an isolated Overframe login, then validate them.".
-spec store_session([map()]) -> {ok, map()} | {error, binary()}.
store_session(Cookies) ->
    case normalize_cookies(Cookies) of
        {ok, Normalized} ->
            case persist(path(), Normalized) of
                ok -> validate(Normalized);
                {error, Reason} -> {error, format_error("write", Reason)}
            end;
        {error, Reason} -> {error, format_error("import", Reason)}
    end.

-doc "Remove the persisted Overframe browser session.".
-spec logout() -> {ok, map()} | {error, binary()}.
logout() ->
    case clear(path()) of
        ok -> {ok, empty_snapshot(false)};
        {error, Reason} -> {error, format_error("remove", Reason)}
    end.

-doc "Return the configured Overframe session path.".
-spec path() -> file:filename_all().
path() ->
    case application:get_env(retinue_daemon, overframe_account_file) of
        {ok, Path} -> Path;
        undefined -> wf_paths:state_file("overframe-session.json")
    end.

-doc "Return authenticated web-session headers without performing a network check.".
-spec session_headers() -> {ok, [{string(), string()}]} | {error, term()}.
session_headers() ->
    case load(path()) of
        {ok, Cookies} when is_list(Cookies) ->
            Cookie = binary_to_list(cookie_header(Cookies)),
            Csrf = csrf_token(Cookies),
            Headers0 = [{"cookie", Cookie}],
            Headers = case Csrf of
                undefined -> Headers0;
                Token -> [{"x-csrftoken", binary_to_list(Token)} | Headers0]
            end,
            {ok, Headers};
        {ok, undefined} -> {error, not_authenticated};
        {error, _Reason} = Error -> Error
    end.

validate(Cookies) ->
    case request(cookie_header(Cookies)) of
        {ok, 200, Body} -> account_snapshot(Body);
        {ok, Status, _Body} when Status =:= 401; Status =:= 403 ->
            {ok, empty_snapshot(true)};
        {ok, Status, _Body} ->
            {error, format_error("check", {http_status, Status})};
        {error, Reason} -> {error, format_error("check", Reason)}
    end.

account_snapshot(#{<<"user">> := User}) when is_map(User) ->
    {ok, #{<<"authenticated">> => true,
           <<"stale">> => false,
           <<"profile">> => User,
           <<"checked_at">> => erlang:system_time(millisecond)}};
account_snapshot(#{<<"user">> := null}) -> {ok, empty_snapshot(true)};
account_snapshot(_Body) -> {error, <<"Overframe session check returned malformed data">>}.

empty_snapshot(Stale) ->
    #{<<"authenticated">> => false,
      <<"stale">> => Stale,
      <<"profile">> => null,
      <<"checked_at">> => erlang:system_time(millisecond)}.

request(CookieHeader) ->
    Headers = [{"cookie", binary_to_list(CookieHeader)}],
    wf_overframe_http:get_json(?AUTH_URL, Headers).

normalize_cookies(Cookies) when is_list(Cookies), length(Cookies) =< 64 ->
    Now = erlang:system_time(second),
    case lists:foldl(fun(Cookie, Acc) -> normalize_cookie(Cookie, Now, Acc) end,
                     {ok, #{}}, Cookies) of
        {ok, ByName} when map_size(ByName) > 0 ->
            case maps:is_key(<<"sessionid">>, ByName) of
                true -> {ok, lists:sort(maps:values(ByName))};
                false -> {error, missing_session_cookie}
            end;
        {ok, _Empty} -> {error, missing_session_cookie};
        {error, _Reason} = Error -> Error
    end;
normalize_cookies(_Cookies) -> {error, invalid_cookie_list}.

normalize_cookie(_Cookie, _Now, {error, _Reason} = Error) -> Error;
normalize_cookie(Cookie, Now, {ok, Acc}) when is_map(Cookie) ->
    Name = maps:get(<<"name">>, Cookie, undefined),
    Value = maps:get(<<"value">>, Cookie, undefined),
    Domain = normalize_domain(maps:get(<<"domain">>, Cookie, undefined)),
    Expires = maps:get(<<"expires">>, Cookie, -1),
    case valid_cookie(Name, Value, Domain, Expires) of
        true when Expires =< 0; Expires > Now ->
            Stored = #{<<"name">> => Name, <<"value">> => Value,
                       <<"domain">> => Domain, <<"expires">> => Expires},
            {ok, Acc#{Name => Stored}};
        true -> {ok, Acc};
        false -> {error, invalid_cookie}
    end;
normalize_cookie(_Cookie, _Now, _Acc) -> {error, invalid_cookie}.

normalize_domain(Domain) when is_binary(Domain) -> string:lowercase(Domain);
normalize_domain(_Domain) -> undefined.

valid_cookie(Name, Value, Domain, Expires) ->
    is_binary(Name) andalso byte_size(Name) > 0 andalso byte_size(Name) =< 128 andalso
    lists:all(fun valid_cookie_name_char/1, binary_to_list(Name)) andalso
    is_binary(Value) andalso byte_size(Value) =< 4096 andalso
    binary:match(Value, <<";">>) =:= nomatch andalso
    binary:match(Value, <<"\r">>) =:= nomatch andalso
    binary:match(Value, <<"\n">>) =:= nomatch andalso
    (Domain =:= <<"overframe.gg">> orelse Domain =:= <<".overframe.gg">>) andalso
    (is_integer(Expires) orelse is_float(Expires)).

valid_cookie_name_char(Char) when Char >= $a, Char =< $z -> true;
valid_cookie_name_char(Char) when Char >= $A, Char =< $Z -> true;
valid_cookie_name_char(Char) when Char >= $0, Char =< $9 -> true;
valid_cookie_name_char(Char) -> lists:member(Char, "!#$%&'*+-.^_`|~").

cookie_header(Cookies) ->
    iolist_to_binary(lists:join(
      <<"; ">>,
      [[maps:get(<<"name">>, Cookie), $=, maps:get(<<"value">>, Cookie)]
       || Cookie <- Cookies])).

csrf_token(Cookies) ->
    case [maps:get(<<"value">>, Cookie)
          || Cookie <- Cookies,
             maps:get(<<"name">>, Cookie, undefined) =:= <<"csrftoken">>] of
        [Token | _] -> Token;
        [] -> undefined
    end.

load(Path) ->
    case file:read_file(Path) of
        {ok, Binary} ->
            _ = file:change_mode(Path, 8#600),
            try jsone:decode(Binary, [{object_format, map}]) of
                #{<<"cookies">> := Cookies} -> normalize_cookies(Cookies);
                _Other -> {error, invalid_session_file}
            catch error:Reason -> {error, {invalid_session_json, Reason}}
            end;
        {error, enoent} -> {ok, undefined};
        {error, Reason} -> {error, Reason}
    end.

persist(Path, Cookies) ->
    case filelib:ensure_dir(Path) of
        ok ->
            Temp = Path ++ ".tmp",
            Data = jsone:encode(#{<<"cookies">> => Cookies,
                                  <<"updated_at">> => erlang:system_time(millisecond)}),
            case file:write_file(Temp, Data, [binary]) of
                ok ->
                    _ = file:change_mode(Temp, 8#600),
                    case file:rename(Temp, Path) of
                        ok -> file:change_mode(Path, 8#600);
                        {error, _Reason} = Error -> Error
                    end;
                {error, _Reason} = Error -> Error
            end;
        {error, _Reason} = Error -> Error
    end.

clear(Path) ->
    _ = file:delete(Path ++ ".tmp"),
    case file:delete(Path) of
        ok -> ok;
        {error, enoent} -> ok;
        {error, _Reason} = Error -> Error
    end.

format_error(Action, Reason) ->
    iolist_to_binary(io_lib:format("Could not ~s Overframe session: ~p",
                                   [Action, Reason])).
