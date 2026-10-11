-module(retinue_paths).

-include_lib("kernel/include/file.hrl").

-export([base/1, root/1, game/2, file/2, game_file/3,
         ensure_private/1, ensure_runtime/0, uid/0]).

base(config) -> env_path("XDG_CONFIG_HOME", ".config");
base(data) -> env_path("XDG_DATA_HOME", ".local/share");
base(state) -> env_path("XDG_STATE_HOME", ".local/state");
base(cache) -> env_path("XDG_CACHE_HOME", ".cache").

root(runtime) ->
    case absolute_env("XDG_RUNTIME_DIR") of
        undefined -> "/tmp/retinue-" ++ integer_to_list(uid());
        Base -> filename:join(Base, "retinue")
    end;
root(Kind) -> filename:join(base(Kind), "retinue").

game(Kind, Game) when Game =:= warframe; Game =:= poe2; Game =:= arc ->
    filename:join([root(Kind), "games", atom_to_list(Game)]).

file(Kind, Name) -> filename:join(root(Kind), Name).
game_file(Kind, Game, Name) -> filename:join(game(Kind, Game), Name).

ensure_private(Path) ->
    case file:read_file_info(Path) of
        {ok, #file_info{type = directory}} -> ok;
        {ok, _} -> {error, enotdir};
        {error, enoent} ->
            case ensure_private(filename:dirname(Path)) of
                ok ->
                    case file:make_dir(Path) of
                        ok -> file:change_mode(Path, 8#700);
                        {error, eexist} -> ensure_private(Path);
                        Error -> Error
                    end;
                Error -> Error
            end;
        Error -> Error
    end.

ensure_runtime() ->
    Path = root(runtime),
    case absolute_env("XDG_RUNTIME_DIR") of
        undefined -> make_runtime(Path);
        Base ->
            case check_runtime(Base) of
                ok -> make_runtime(Path);
                Error -> Error
            end
    end.

make_runtime(Path) ->
    case file:make_dir(Path) of
        ok ->
            case file:change_mode(Path, 8#700) of
                ok -> check_runtime(Path);
                Error -> Error
            end;
        {error, eexist} -> check_runtime(Path);
        Error -> Error
    end.

check_runtime(Path) ->
    Uid = uid(),
    case file:read_link_info(Path) of
        {ok, #file_info{type = directory, uid = Uid, mode = Mode}}
          when Mode band 8#777 =:= 8#700 -> ok;
        {ok, _} -> {error, {unsafe_runtime_directory, Path}};
        Error -> Error
    end.

uid() ->
    {ok, #file_info{uid = Uid}} = file:read_file_info("/proc/self"),
    Uid.

env_path(Name, Suffix) ->
    case absolute_env(Name) of
        undefined -> filename:join(home(), Suffix);
        Path -> Path
    end.

absolute_env(Name) ->
    case os:getenv(Name) of
        "/" ++ _ = Path -> Path;
        _ -> undefined
    end.

home() ->
    case absolute_env("HOME") of
        undefined ->
            {ok, [[Home]]} = init:get_argument(home),
            filename:absname(Home);
        Home -> Home
    end.
