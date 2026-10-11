-module(wf_test_cli).

-export([parse/1, result/1, help/1, candidates/1, tree/0, catalog/2, worldstate/1, options/1]).

parse(Args) ->
    {ok, Parsed, _Path, _Command} = result(Args),
    Parsed.

tree() -> (wf_cli:command())#{arguments => maps:get(arguments, retinue_cli:command())}.
result(Args) -> retinue_cli_args:parse(Args, tree(), "wfcli").
help(Path) -> scoped(fun() -> retinue_help:text(Path) end).
candidates(Words) -> retinue_completion:candidates(["warframe" | Words]).

scoped(Run) ->
    retinue_cli_runtime:with_context(#{tree => tree(), progname => "wfcli",
        groups => [{Name, [Cmd || {Cmd, _} <- Rows]} || {Name, Rows} <- wf_cli:command_groups()]}, Run).

catalog(Command, Args) -> wf_catalog_cli:request(parse([Command | Args])).

worldstate(Args) -> wf_worldstate_cli:prepare(parse(Args)).

options(Path) ->
    retinue_cli_args:options(retinue_cli_args:node(Path, tree())).
