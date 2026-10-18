%%%-------------------------------------------------------------------
%% HTTP boundary for the private Overframe web API and static data.
%%%-------------------------------------------------------------------
-module(wf_overframe_http).

-export([get/2, get_json/2]).

-define(TIMEOUT, 30000).

-doc "Fetch a URL and retain non-2xx status codes for adapter decisions.".
-spec get(string() | binary(), [{string(), string()}]) ->
    {ok, non_neg_integer(), binary()} | {error, term()}.
get(Url0, Headers0) ->
    Url = retinue_text:to_list(Url0),
    Headers = default_headers(Headers0),
    Result = try
        case application:get_env(retinue_daemon, overframe_http_fun) of
            {ok, Fun} when is_function(Fun, 2) -> Fun(Url, Headers);
            _ -> real_get(Url, Headers)
        end
    catch Class:Reason -> {error, {http_crash, Class, Reason}}
    end,
    normalize(Result).

-doc "Fetch and decode one JSON object or array.".
-spec get_json(string() | binary(), [{string(), string()}]) ->
    {ok, non_neg_integer(), map() | list()} | {error, term()}.
get_json(Url, Headers) ->
    case get(Url, Headers) of
        {ok, Status, Body} ->
            try jsone:decode(Body, [{object_format, map}]) of
                Data when is_map(Data); is_list(Data) -> {ok, Status, Data};
                Other -> {error, {invalid_json_root, Other}}
            catch error:Reason -> {error, {invalid_json, Reason}}
            end;
        {error, _Reason} = Error -> Error
    end.

real_get(Url, Headers) ->
    case httpc:request(get, {Url, Headers}, [{timeout, ?TIMEOUT}],
                       [{body_format, binary}]) of
        {ok, {{_Version, Status, _Reason}, _ResponseHeaders, Body}} ->
            {ok, Status, Body};
        {error, Reason} -> {error, Reason}
    end.

normalize({ok, Status, Body}) when is_integer(Status), is_binary(Body) ->
    {ok, Status, Body};
normalize({ok, Status, _Headers, Body}) when is_integer(Status), is_binary(Body) ->
    {ok, Status, Body};
normalize({error, _Reason} = Error) -> Error;
normalize(Other) -> {error, {invalid_http_result, Other}}.

default_headers(Headers) ->
    add_header("user-agent", user_agent(),
               add_header("accept", "application/json", Headers)).

add_header(Name, Value, Headers) ->
    case lists:keymember(Name, 1, Headers) of
        true -> Headers;
        false -> [{Name, Value} | Headers]
    end.

user_agent() ->
    "retinue/" ++ retinue_build:version() ++
        " (+https://github.com/ZeeWanderer/retinue)".
