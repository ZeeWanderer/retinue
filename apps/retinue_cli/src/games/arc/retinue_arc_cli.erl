-module(retinue_arc_cli).
-export([command/0]).

command() ->
    #{help => "ARC Raiders launch scaffold (observation not implemented)",
      commands => #{"status" => #{help => "show capabilities and installed game path",
          handler => fun(_) -> status() end}}}.

status() ->
    case retinue_native_program:run(arccompanion, ["status", "--json"]) of
        {ok, Output} ->
            Data = jsone:decode(Output),
            retinue_output:emit(Data, fun() ->
                io:put_chars("ARC Raiders: launch only; gameplay observation is not implemented.\n"),
                case maps:get(<<"install_directory">>, Data, null) of
                    null -> io:put_chars("  installation: not found\n");
                    Path -> io:format("  installed: ~ts~n", [Path])
                end
            end);
        {error, Reason} -> retinue_cli_runtime:fail(retinue_text:to_list(Reason))
    end.
