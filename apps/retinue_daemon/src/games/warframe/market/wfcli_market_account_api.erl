%%%-------------------------------------------------------------------
%% Authenticated Warframe.market HTTP contract.
%%%-------------------------------------------------------------------
-module(wfcli_market_account_api).

-export([login/2, profile/1, orders/1, create_order/2, update_order/3,
         delete_order/2, close_order/3, set_group_visibility/3]).

-define(BASE_URL, "https://api.warframe.market").

-doc "Use the temporary legacy v1 authorization flow and return its JWT.".
-spec login(binary(), binary()) -> {ok, binary()} | {error, term()}.
login(Email, Password) when is_binary(Email), is_binary(Password) ->
    Body = #{<<"auth_type">> => <<"header">>, <<"email">> => Email,
             <<"password">> => Password},
    case request(post, ?BASE_URL ++ "/v1/auth/signin", login_headers(), Body) of
        {ok, _Status, Headers, _Data} ->
            case response_token(Headers) of
                undefined -> {error, missing_market_token};
                Token -> {ok, Token}
            end;
        {error, {market_account_http_status, 400, Error}} ->
            case credential_error(Error) of
                true -> {error, invalid_market_credentials};
                false -> {error, {market_account_login_failed, Error}}
            end;
        {error, _Reason} = Error -> Error
    end.

-doc "Fetch the authenticated private profile and any rotated JWT.".
-spec profile(binary()) -> {ok, map(), binary() | undefined} | {error, term()}.
profile(Token) -> authenticated(get, "/v2/me", Token, undefined).

-doc "Fetch all visible and hidden orders owned by the account.".
-spec orders(binary()) -> {ok, [map()], binary() | undefined} | {error, term()}.
orders(Token) ->
    case authenticated(get, "/v2/orders/my", Token, undefined) of
        {ok, Orders, Rotated} when is_list(Orders) -> {ok, Orders, Rotated};
        {ok, _Other, _Rotated} -> {error, invalid_market_orders};
        {error, _Reason} = Error -> Error
    end.

-doc "Create one regular item order.".
-spec create_order(binary(), map()) -> {ok, map(), binary() | undefined} | {error, term()}.
create_order(Token, Order) -> authenticated(post, "/v2/order", Token, Order).

-doc "Patch mutable fields on one owned order.".
-spec update_order(binary(), binary(), map()) ->
    {ok, map(), binary() | undefined} | {error, term()}.
update_order(Token, Id, Patch) ->
    authenticated(patch, "/v2/order/" ++ path_segment(Id), Token, Patch).

-doc "Delete one owned order.".
-spec delete_order(binary(), binary()) ->
    {ok, map(), binary() | undefined} | {error, term()}.
delete_order(Token, Id) ->
    authenticated(delete, "/v2/order/" ++ path_segment(Id), Token, undefined).

-doc "Close a quantity from one owned order.".
-spec close_order(binary(), binary(), pos_integer()) ->
    {ok, map(), binary() | undefined} | {error, term()}.
close_order(Token, Id, Quantity) ->
    authenticated(post, "/v2/order/" ++ path_segment(Id) ++ "/close", Token,
                  #{<<"quantity">> => Quantity}).

-doc "Change visibility for every regular order or one order side.".
-spec set_group_visibility(binary(), boolean(), binary() | undefined) ->
    {ok, map(), binary() | undefined} | {error, term()}.
set_group_visibility(Token, Visible, Type) ->
    Body0 = #{<<"visible">> => Visible},
    Body = case Type of
        <<"buy">> -> Body0#{<<"type">> => Type};
        <<"sell">> -> Body0#{<<"type">> => Type};
        _ -> Body0
    end,
    authenticated(patch, "/v2/orders/group/all", Token, Body).

authenticated(Method, Path, Token, Body) ->
    case request(Method, ?BASE_URL ++ Path, auth_headers(Token), Body) of
        {ok, _Status, Headers, Envelope} ->
            case envelope_data(Envelope) of
                {ok, Data} -> {ok, Data, response_token(Headers)};
                {error, _Reason} = Error -> Error
            end;
        {error, {market_account_http_status, Status, _Error}}
          when Status =:= 401; Status =:= 403 ->
            {error, market_account_unauthorized};
        {error, _Reason} = Error -> Error
    end.

request(Method, Url, Headers, Body) ->
    ok = wfcli_market_limiter:wait(),
    Encoded = case Body of undefined -> <<>>; _ -> jsone:encode(Body) end,
    Result = try
        case application:get_env(retinue_daemon, market_account_http_fun) of
            {ok, Fun} when is_function(Fun, 4) -> Fun(Method, Url, Headers, Encoded);
            _ -> real_request(Method, Url, Headers, Encoded)
        end
    catch Class:Reason -> {error, {market_account_http_crash, Class, Reason}}
    end,
    normalize_response(Result).

real_request(Method, Url, Headers, Body) ->
    Request = case Method of
        get -> {Url, Headers};
        delete -> {Url, Headers};
        _ -> {Url, Headers, "application/json", Body}
    end,
    case httpc:request(Method, Request, [{timeout, 15000}], [{body_format, binary}]) of
        {ok, {{_Version, Status, _Reason}, ResponseHeaders, ResponseBody}} ->
            {ok, Status, ResponseHeaders, ResponseBody};
        {error, Reason} -> {error, Reason}
    end.

normalize_response({ok, Status, Headers, Body}) when Status >= 200, Status < 300 ->
    case decode_body(Body) of
        {ok, Data} -> {ok, Status, Headers, Data};
        {error, _Reason} = Error -> Error
    end;
normalize_response({ok, Status, _Headers, Body}) ->
    Error = case decode_body(Body) of
        {ok, Data} -> Data;
        {error, _Reason} -> <<>>
    end,
    {error, {market_account_http_status, Status, Error}};
normalize_response({error, Reason}) -> {error, {market_account_http_failed, Reason}};
normalize_response(Other) -> {error, {invalid_market_account_http_result, Other}}.

decode_body(<<>>) -> {ok, #{}};
decode_body(Body) ->
    try jsone:decode(iolist_to_binary(Body), [{object_format, map}]) of
        Data -> {ok, Data}
    catch error:Reason -> {error, {invalid_market_account_json, Reason}}
    end.

envelope_data(#{<<"error">> := Error}) when Error =/= null ->
    {error, {market_account_api_error, Error}};
envelope_data(#{<<"data">> := Data}) -> {ok, Data};
envelope_data(_Envelope) -> {error, invalid_market_account_envelope}.

login_headers() ->
    [{"authorization", "JWT"}, {"auth_type", "header"} | common_headers()].

auth_headers(Token) ->
    [{"authorization", "Bearer " ++ binary_to_list(Token)},
     {"auth_type", "header"} | common_headers()].

common_headers() ->
    [{"user-agent", "wfcli/0.1 (+https://github.com/ZeeWanderer/wfcli)"},
     {"accept", "application/json"}, {"language", "en"},
     {"platform", "pc"}, {"crossplay", "true"},
     {"content-type", "application/json"}].

response_token(Headers) ->
    Values = [retinue_text:to_list(Value) || {Name, Value} <- Headers,
              string:lowercase(retinue_text:to_list(Name)) =:= "authorization"],
    case Values of
        [Value | _] -> strip_token_prefix(string:trim(Value));
        [] -> undefined
    end.

strip_token_prefix(Value) ->
    case string:split(Value, " ", leading) of
        [Prefix, Token] ->
            case string:lowercase(Prefix) of
                "jwt" -> list_to_binary(string:trim(Token));
                "bearer" -> list_to_binary(string:trim(Token));
                _ -> undefined
            end;
        _ -> undefined
    end.

credential_error(Error) ->
    Text = retinue_text:to_list(io_lib:format("~p", [Error])),
    lists:any(fun(Code) -> string:find(Text, Code) =/= nomatch end,
              ["app.form.invalid", "app.account.password_invalid",
               "app.account.email_not_exist"]).

path_segment(Value) -> uri_string:quote(retinue_text:to_list(Value)).
