%%%-------------------------------------------------------------------
%% Owner-only persistence for the Warframe.market session token.
%%%-------------------------------------------------------------------
-module(wfcli_market_account_store).

-export([path/0, load/1, persist/2, clear/1]).

-doc "Return configured Market account token path.".
-spec path() -> file:filename_all().
path() ->
    case application:get_env(retinue_daemon, market_account_file) of
        {ok, Path} -> Path;
        undefined -> wfcli_paths:state_file("market-token")
    end.

-doc "Read a non-empty token and tighten existing file permissions.".
-spec load(file:filename_all()) -> binary() | undefined.
load(Path) ->
    case file:read_file(Path) of
        {ok, Binary} ->
            Token = string:trim(Binary),
            _ = file:change_mode(Path, 8#600),
            case Token of <<>> -> undefined; _ -> Token end;
        {error, _Reason} -> undefined
    end.

-doc "Atomically persist a token with mode 0600.".
-spec persist(file:filename_all(), binary()) -> ok | {error, term()}.
persist(Path, Token) when is_binary(Token), byte_size(Token) > 0 ->
    case filelib:ensure_dir(Path) of
        ok ->
            Temp = Path ++ ".tmp",
            case file:write_file(Temp, Token, [binary]) of
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

-doc "Remove the persisted session token.".
-spec clear(file:filename_all()) -> ok | {error, term()}.
clear(Path) ->
    _ = file:delete(Path ++ ".tmp"),
    case file:delete(Path) of
        ok -> ok;
        {error, enoent} -> ok;
        {error, _Reason} = Error -> Error
    end.
