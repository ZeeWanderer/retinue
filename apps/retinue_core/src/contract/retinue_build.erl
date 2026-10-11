%%%-------------------------------------------------------------------
%% Runtime identity and staged artifact locations.
%%%-------------------------------------------------------------------
-module(retinue_build).

-export([version/0, flavor/0, installation/0, install_root/0, update_root/0, homebrew_install/0,
         hot_ebin_dirs/0, artifact_id/0]).

-ifndef(RETINUE_VERSION).
-define(RETINUE_VERSION, "unknown").
-endif.

-ifdef(TEST).
-export([flavor_from_path/1, brew_update_root/1, homebrew_install_from_path/1]).
-endif.

-type flavor() :: dev | prod.
-type installation() :: homebrew | local.

-doc "Return release version compiled from root VERSION.".
-spec version() -> string().
version() -> ?RETINUE_VERSION.

-doc "Return artifact flavor selected by environment or executable path.".
-spec flavor() -> flavor().
flavor() ->
    case os:getenv("RETINUE_BUILD_FLAVOR") of
        "dev" -> dev;
        "prod" -> prod;
        _ -> flavor_from_path(executable_path())
    end.

-doc "Return whether this artifact came from Homebrew or a local build.".
-spec installation() -> installation().
installation() ->
    case homebrew_install() of
        true -> homebrew;
        false -> local
    end.

-doc "Return current staged installation root.".
-spec install_root() -> file:filename_all().
install_root() ->
    case os:getenv("RETINUE_INSTALL_ROOT") of
        Value when is_list(Value), Value =/= "" -> absolute_path(Value);
        _ -> root_from_executable(executable_path())
    end.

-doc "Return stable artifact root watched by a running daemon.".
-spec update_root() -> file:filename_all().
update_root() ->
    case os:getenv("RETINUE_UPDATE_ROOT") of
        Value when is_list(Value), Value =/= "" -> absolute_path(Value);
        _ -> brew_update_root(install_root())
    end.

-doc "Return whether the running artifact is installed by Homebrew.".
-spec homebrew_install() -> boolean().
homebrew_install() ->
    case os:getenv("RETINUE_PACKAGE_MANAGER") of
        "homebrew" -> true;
        "standalone" -> false;
        _ -> homebrew_install_from_path(install_root())
    end.

-doc "Find hot-loadable wfcore and retinue_daemon ebin directories in current artifact.".
-spec hot_ebin_dirs() -> {ok, [file:filename_all()]} | {error, term()}.
hot_ebin_dirs() ->
    find_ebin_dirs([retinue_core, wfcore, retinue_daemon], []).

find_ebin_dirs([], Acc) -> {ok, lists:reverse(Acc)};
find_ebin_dirs([App | Rest], Acc) ->
    Pattern = filename:join([update_root(), "libexec", "retinue_daemon", "lib",
                             atom_to_list(App) ++ "-*", "ebin"]),
    case lists:sort(filelib:wildcard(Pattern)) of
        [] -> {error, {application_ebin_not_found, App, Pattern}};
        Dirs -> find_ebin_dirs(Rest, [lists:last(Dirs) | Acc])
    end.

-doc "Read staged artifact marker used by daemon self-update checks.".
-spec artifact_id() -> {ok, binary()} | {error, term()}.
artifact_id() ->
    Path = filename:join(update_root(), "BUILD_ID"),
    case file:read_file(Path) of
        {ok, Binary} -> {ok, string:trim(Binary)};
        {error, Reason} -> {error, {artifact_id_unavailable, Path, Reason}}
    end.

flavor_from_path(Path) ->
    Parts = filename:split(Path),
    DevAliases = ["retinued", "wfclid", "poe2clid", "arcclid"],
    case lists:member("dev", Parts) orelse lists:member(filename:basename(Path), DevAliases) of
        true -> dev;
        false -> prod
    end.

root_from_executable(Path) ->
    Resolved = resolve_link(filename:absname(Path), 8),
    case filename:basename(filename:dirname(Resolved)) of
        "bin" -> filename:dirname(filename:dirname(Resolved));
        _ -> filename:absname(".")
    end.

brew_update_root(Root) ->
    AbsoluteRoot = absolute_path(Root),
    Parts = filename:split(AbsoluteRoot),
    case split_at_cellar(Parts, []) of
        {ok, Prefix, Formula} -> filename:join(Prefix ++ ["opt", Formula]);
        error -> AbsoluteRoot
    end.

homebrew_install_from_path(Path) ->
    Absolute = filename:absname(Path),
    homebrew_retinue_path(Absolute) orelse
        homebrew_retinue_path(resolve_link(Absolute, 8)).

homebrew_retinue_path(Path) ->
    homebrew_retinue_parts(filename:split(Path)).

homebrew_retinue_parts(["Cellar", "retinue", _Version | _Rest]) -> true;
homebrew_retinue_parts(["opt", "retinue" | _Rest]) -> true;
homebrew_retinue_parts([_Part | Rest]) -> homebrew_retinue_parts(Rest);
homebrew_retinue_parts([]) -> false.

absolute_path(Path) ->
    case filename:pathtype(Path) of
        absolute -> Path;
        _ -> filename:absname(Path)
    end.

split_at_cellar(["Cellar", Formula, _Version | _Rest], Prefix) ->
    {ok, lists:reverse(Prefix), Formula};
split_at_cellar([Part | Rest], Prefix) ->
    split_at_cellar(Rest, [Part | Prefix]);
split_at_cellar([], _Prefix) -> error.

executable_path() ->
    try escript:script_name() of
        Name when is_list(Name), Name =/= "" -> Name;
        _ -> filename:absname(".")
    catch
        _:_ -> filename:absname(".")
    end.

resolve_link(Path, 0) -> Path;
resolve_link(Path, Remaining) ->
    case file:read_link(Path) of
        {ok, Target} ->
            Next = case filename:pathtype(Target) of
                absolute -> Target;
                _ -> filename:join(filename:dirname(Path), Target)
            end,
            resolve_link(filename:absname(Next), Remaining - 1);
        {error, _Reason} -> Path
    end.
