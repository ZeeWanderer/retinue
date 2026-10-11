%%%-------------------------------------------------------------------
%% Reversible Steam launch-option setup for wfcompanion.
%%%-------------------------------------------------------------------
-module(wfcli_companion_steam).

-include_lib("kernel/include/file.hrl").

-export([install/2, uninstall/1, plan/2, restore/3, steam_running/0]).

-ifdef(TEST).
-export([dedupe_configs/1]).
-endif.

-define(APP_ID, <<"230410">>).
-define(STATE_FILE, "companion-steam.term").

-doc "Install wfcompanion as Warframe's Steam launch wrapper.".
-spec install(file:filename_all(), boolean()) -> {ok, map()} | {error, term()}.
install(CompanionPath, DryRun) ->
    with_config(
      fun(Config, Content) ->
          case plan(Content, CompanionPath) of
              {ok, Plan} -> maybe_install(Config, Content, Plan, DryRun);
              {error, _Reason} = Error -> Error
          end
      end).

-doc "Restore launch options saved by install/2.".
-spec uninstall(boolean()) -> {ok, map()} | {error, term()}.
uninstall(DryRun) ->
    case read_state() of
        {ok, #{config := Config, original := Original, installed := Installed} = State} ->
            case file:read_file(Config) of
                {ok, Content} ->
                    case restore(Content, Installed, Original) of
                        {ok, Restored} -> maybe_uninstall(State, Content, Restored, DryRun);
                        {error, _Reason} = Error -> Error
                    end;
                {error, Reason} -> {error, {steam_config_read_failed, Config, Reason}}
            end;
        {error, _Reason} = Error -> Error
    end.

-doc "Build a launch-option update without writing it.".
-spec plan(binary(), file:filename_all()) -> {ok, map()} | {error, term()}.
plan(Content, CompanionPath) when is_binary(Content) ->
    case launch_options_span(Content) of
        {ok, Start, End, Current} ->
            Proposed = launch_options(Current, CompanionPath),
            {ok, #{current => Current,
                   proposed => Proposed,
                   content => replace(Content, Start, End, vdf_escape(Proposed))}};
        {error, _Reason} = Error -> Error
    end.

-doc "Restore an exact installed launch-option value.".
-spec restore(binary(), binary(), binary()) -> {ok, binary()} | {error, term()}.
restore(Content, Installed, Original) ->
    case launch_options_span(Content) of
        {ok, Start, End, Installed} ->
            {ok, replace(Content, Start, End, vdf_escape(Original))};
        {ok, _Start, _End, Current} ->
            {error, {launch_options_changed, Current}};
        {error, _Reason} = Error -> Error
    end.

-doc "Return true while a Steam client process exists.".
-spec steam_running() -> boolean().
steam_running() ->
    case file:list_dir("/proc") of
        {ok, Entries} ->
            lists:any(
              fun(Entry) ->
                  case all_digits(Entry) of
                      true -> steam_process(filename:join(["/proc", Entry, "comm"]));
                      false -> false
                  end
              end,
              Entries);
        {error, _Reason} -> false
    end.

maybe_install(Config, _Content, Plan, true) ->
    {ok, Plan#{config => Config, dry_run => true}};
maybe_install(Config, Content, Plan, false) ->
    case filelib:is_regular(state_path()) of
        true -> {error, companion_already_installed};
        false -> maybe_write_install(Config, Content, Plan)
    end.

maybe_write_install(Config, Content, Plan) ->
    case steam_running() of
        true -> {error, steam_running};
        false ->
            State = #{config => Config,
                      original => maps:get(current, Plan),
                      installed => maps:get(proposed, Plan)},
            case write_state(State) of
                ok ->
                    case atomic_write(Config, Content, maps:get(content, Plan)) of
                        ok -> {ok, Plan#{config => Config, dry_run => false}};
                        {error, _Reason} = Error ->
                            _ = file:delete(state_path()),
                            Error
                    end;
                {error, _Reason} = Error -> Error
            end
    end.

maybe_uninstall(State, _Content, Restored, true) ->
    {ok, State#{content => Restored, dry_run => true}};
maybe_uninstall(#{config := Config} = State, Content, Restored, false) ->
    case steam_running() of
        true -> {error, steam_running};
        false ->
            case atomic_write(Config, Content, Restored) of
                ok ->
                    case file:delete(state_path()) of
                        ok -> {ok, State#{dry_run => false}};
                        {error, enoent} -> {ok, State#{dry_run => false}};
                        {error, Reason} -> {error, {state_delete_failed, Reason}}
                    end;
                {error, _Reason} = Error -> Error
            end
    end.

with_config(Fun) ->
    case find_configs() of
        [Config] ->
            case file:read_file(Config) of
                {ok, Content} -> Fun(Config, Content);
                {error, Reason} -> {error, {steam_config_read_failed, Config, Reason}}
            end;
        [] -> {error, warframe_steam_config_not_found};
        Configs -> {error, {multiple_warframe_steam_configs, Configs}}
    end.

find_configs() ->
    Candidates = case os:getenv("WFCLI_STEAM_LOCALCONFIG") of
        false -> default_config_candidates();
        "" -> default_config_candidates();
        Path -> [filename:absname(Path)]
    end,
    dedupe_configs(
      [Path || Path <- Candidates,
               filelib:is_regular(Path),
               config_has_warframe(Path)]).

dedupe_configs(Paths) ->
    {_Identities, Configs} =
        lists:foldl(
          fun(Path, {Seen, Acc}) ->
              Identity = config_identity(Path),
              case sets:is_element(Identity, Seen) of
                  true -> {Seen, Acc};
                  false -> {sets:add_element(Identity, Seen), [Path | Acc]}
              end
          end,
          {sets:new(), []},
          lists:usort(Paths)),
    lists:reverse(Configs).

config_identity(Path) ->
    case file:read_file_info(Path) of
        {ok, #file_info{inode = Inode,
                        major_device = Major,
                        minor_device = Minor}} when Inode =/= 0 ->
            {file, Major, Minor, Inode};
        {ok, _Info} ->
            {path, filename:absname(Path)};
        {error, _Reason} ->
            {path, filename:absname(Path)}
    end.

default_config_candidates() ->
    Home = case os:getenv("HOME") of false -> "."; Value -> Value end,
    Roots = [filename:join([Home, ".steam", "steam", "userdata"]),
             filename:join([Home, ".local", "share", "Steam", "userdata"])],
    lists:append(
      [filelib:wildcard(filename:join([Root, "*", "config", "localconfig.vdf"]))
       || Root <- Roots]).

config_has_warframe(Path) ->
    case file:read_file(Path) of
        {ok, Content} ->
            case launch_options_span(Content) of
                {ok, _Start, _End, _Current} -> true;
                {error, _Reason} -> false
            end;
        {error, _Reason} -> false
    end.

launch_options(Current, CompanionPath) ->
    Prefix = iolist_to_binary([shell_quote(filename:absname(CompanionPath)), " launch -- "]),
    case string:trim(Current) of
        <<>> -> <<Prefix/binary, "%command%">>;
        Trimmed ->
            case existing_wrapper_payload(Trimmed) of
                {ok, Payload} -> <<Prefix/binary, Payload/binary>>;
                no ->
                    case binary:match(Trimmed, <<"%command%">>) of
                        nomatch -> <<Prefix/binary, "%command% ", Trimmed/binary>>;
                        _ -> <<Prefix/binary, Trimmed/binary>>
                    end
            end
    end.

existing_wrapper_payload(Options) ->
    Pattern = <<"^(?:'[^']*wfcompanion'|[^ ]*wfcompanion) launch -- (.+)$">>,
    case re:run(Options, Pattern, [{capture, [1], binary}]) of
        {match, [Payload]} -> {ok, Payload};
        nomatch -> no
    end.

shell_quote(Path) ->
    Escaped = string:replace(Path, "'", "'\\''", all),
    ["'", Escaped, "'"].

launch_options_span(Content) ->
    AppPattern = <<"\"", ?APP_ID/binary, "\"">>,
    find_app_block(Content, AppPattern, 0).

find_app_block(Content, Pattern, From) when From < byte_size(Content) ->
    case match_between(Content, Pattern, From, byte_size(Content)) of
        nomatch -> {error, warframe_app_block_not_found};
        {AppAt, AppLength} ->
            AfterKey = skip_space(Content, AppAt + AppLength),
            case byte_at(Content, AfterKey) of
                ${ ->
                    case matching_brace(Content, AfterKey) of
                        {ok, BlockEnd} -> find_launch_options(Content, AfterKey + 1, BlockEnd);
                        error -> {error, invalid_warframe_app_block}
                    end;
                _ -> find_app_block(Content, Pattern, AppAt + AppLength)
            end
    end;
find_app_block(_Content, _Pattern, _From) -> {error, warframe_app_block_not_found}.

find_launch_options(Content, Start, End) ->
    Pattern = <<"\"LaunchOptions\"">>,
    case match_between(Content, Pattern, Start, End) of
        nomatch -> {error, launch_options_not_found};
        {At, Length} ->
            ValueAt = skip_space(Content, At + Length),
            case quoted_value(Content, ValueAt, End) of
                {ok, ValueStart, ValueEnd, Value} -> {ok, ValueStart, ValueEnd, Value};
                error -> {error, invalid_launch_options}
            end
    end.

matching_brace(Content, Open) -> matching_brace(Content, Open + 1, 1, normal).

matching_brace(Content, Pos, _Depth, _State) when Pos >= byte_size(Content) -> error;
matching_brace(Content, Pos, Depth, normal) ->
    case binary:at(Content, Pos) of
        $\" -> matching_brace(Content, Pos + 1, Depth, quoted);
        ${ -> matching_brace(Content, Pos + 1, Depth + 1, normal);
        $} when Depth =:= 1 -> {ok, Pos};
        $} -> matching_brace(Content, Pos + 1, Depth - 1, normal);
        _ -> matching_brace(Content, Pos + 1, Depth, normal)
    end;
matching_brace(Content, Pos, Depth, quoted) ->
    case binary:at(Content, Pos) of
        $\\ -> matching_brace(Content, Pos + 1, Depth, escaped);
        $\" -> matching_brace(Content, Pos + 1, Depth, normal);
        _ -> matching_brace(Content, Pos + 1, Depth, quoted)
    end;
matching_brace(Content, Pos, Depth, escaped) ->
    matching_brace(Content, Pos + 1, Depth, quoted).

quoted_value(Content, QuoteAt, Limit) ->
    case byte_at(Content, QuoteAt) of
        $\" -> quoted_value(Content, QuoteAt + 1, QuoteAt + 1, Limit, []);
        _ -> error
    end.

quoted_value(_Content, Pos, _Start, Limit, _Acc) when Pos >= Limit -> error;
quoted_value(Content, Pos, Start, Limit, Acc) ->
    case binary:at(Content, Pos) of
        $\" -> {ok, Start, Pos, iolist_to_binary(lists:reverse(Acc))};
        $\\ when Pos + 1 < Limit ->
            Escaped = binary:at(Content, Pos + 1),
            quoted_value(Content, Pos + 2, Start, Limit, [Escaped | Acc]);
        Byte -> quoted_value(Content, Pos + 1, Start, Limit, [Byte | Acc])
    end.

skip_space(Content, Pos) when Pos >= byte_size(Content) -> Pos;
skip_space(Content, Pos) ->
    case binary:at(Content, Pos) of
        Byte when Byte =:= $\s; Byte =:= $\t; Byte =:= $\r; Byte =:= $\n ->
            skip_space(Content, Pos + 1);
        _ -> Pos
    end.

match_between(Content, Pattern, Start, End) when Start < End ->
    Length = End - Start,
    case binary:match(Content, Pattern, [{scope, {Start, Length}}]) of
        nomatch -> nomatch;
        Match -> Match
    end;
match_between(_Content, _Pattern, _Start, _End) -> nomatch.

byte_at(Content, Pos) when Pos >= 0, Pos < byte_size(Content) -> binary:at(Content, Pos);
byte_at(_Content, _Pos) -> undefined.

replace(Content, Start, End, Replacement) ->
    Prefix = binary:part(Content, 0, Start),
    Suffix = binary:part(Content, End, byte_size(Content) - End),
    <<Prefix/binary, Replacement/binary, Suffix/binary>>.

vdf_escape(Value) ->
    iolist_to_binary(
      [case Byte of
           $\\ -> <<"\\\\">>;
           $\" -> <<"\\\"">>;
           _ -> <<Byte>>
       end || <<Byte>> <= Value]).

state_path() -> wfcli_paths:config_file(?STATE_FILE).

write_state(State) ->
    Path = state_path(),
    ok = filelib:ensure_dir(Path),
    atomic_write_new(Path, term_to_binary(State)).

read_state() ->
    Path = state_path(),
    case file:read_file(Path) of
        {ok, Binary} ->
            try binary_to_term(Binary, [safe]) of
                #{config := _Config, original := _Original, installed := _Installed} = State ->
                    {ok, State};
                _ -> {error, invalid_companion_steam_state}
            catch
                error:badarg -> {error, invalid_companion_steam_state}
            end;
        {error, enoent} -> {error, companion_not_installed};
        {error, Reason} -> {error, {state_read_failed, Reason}}
    end.

atomic_write(Path, OriginalContent, Content) ->
    Mode = case file:read_file_info(Path) of
        {ok, Info} -> Info#file_info.mode;
        {error, _Reason} -> undefined
    end,
    atomic_write(Path, Content, Mode, OriginalContent).

atomic_write_new(Path, Content) -> atomic_write(Path, Content, undefined, undefined).

atomic_write(Path, Content, Mode, _OriginalContent) ->
    Temp = Path ++ ".wfcli.tmp",
    case file:write_file(Temp, Content) of
        ok ->
            case Mode of undefined -> ok; _ -> file:change_mode(Temp, Mode) end,
            case file:rename(Temp, Path) of
                ok -> ok;
                {error, Reason} ->
                    _ = file:delete(Temp),
                    {error, {atomic_rename_failed, Path, Reason}}
            end;
        {error, Reason} -> {error, {atomic_write_failed, Temp, Reason}}
    end.

steam_process(Path) ->
    case file:read_file(Path) of
        {ok, Name0} ->
            Name = string:trim(Name0),
            Name =:= <<"steam">> orelse Name =:= <<"steamwebhelper">>;
        {error, _Reason} -> false
    end.

all_digits([]) -> false;
all_digits(Value) -> lists:all(fun(Char) -> Char >= $0 andalso Char =< $9 end, Value).
