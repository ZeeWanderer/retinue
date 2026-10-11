-module(retinue_incidents).

-export([path/1, read/3, retained/2]).

path(daemon) -> retinue_paths:file(state, "daemon.log");
path(companion) ->
    case os:getenv("WFCOMPANION_LOG") of
        false -> filename:join(retinue_paths:game(state, warframe), "companion.log");
        Path -> Path
    end.

read(Source, Path, Limit) ->
    Report = #{application => Source, path => unicode:characters_to_binary(Path)},
    case file:open(Path, [read, binary, raw]) of
        {ok, File} ->
            try read_tail(File, Report, Limit)
            after file:close(File) end;
        {error, enoent} -> Report#{entries => [], missing => true};
        {error, Reason} -> Report#{error => file_error(Reason)}
    end.

retained(Source, Path) ->
    Suffixes = case Source of daemon -> [".1", ".0", ""]; companion -> [".1", ""] end,
    [read(Source, retinue_text:to_list(Path) ++ Suffix, infinity) || Suffix <- Suffixes].

read_tail(File, Report, Limit) ->
    case file:position(File, eof) of
        {ok, Size} ->
            Start = max(0, Size - 1024 * 1024),
            case file:pread(File, Start, Size - Start) of
                {ok, Bytes} -> Report#{entries => entries(Bytes, Start, Limit), truncated => Start > 0};
                eof -> Report#{entries => []};
                {error, Reason} -> Report#{error => file_error(Reason)}
            end;
        {error, Reason} -> Report#{error => file_error(Reason)}
    end.

entries(Bytes, Start, Limit) ->
    Lines0 = binary:split(Bytes, <<"\n">>, [global]),
    Lines1 = case Start of 0 -> Lines0; _ -> tl(Lines0) end,
    Lines = [Line || Line <- Lines1, Line =/= <<>>],
    Tail = case Limit of
        infinity -> Lines;
        _ -> lists:nthtail(max(0, length(Lines) - Limit), Lines)
    end,
    [entry(Line) || Line <- Tail].

entry(Line) ->
    try json:decode(Line) of
        #{<<"event">> := Event, <<"message">> := Message} = Entry
          when is_binary(Event), is_binary(Message) -> Entry;
        _ -> plain_entry(Line)
    catch error:_ -> plain_entry(Line) end.

plain_entry(Line) ->
    case re:run(Line, "^(\\S+)\\s+(debug|info|notice|warning|error|critical|alert|emergency):\\s*(.*)$",
                [{capture, all_but_first, binary}]) of
        {match, [Time, Level, Message]} ->
            case retinue_time:parse(Time) of
                {ok, Millis} -> #{<<"timestamp_ms">> => Millis, <<"level">> => Level,
                                  <<"event">> => <<"daemon.log">>, <<"message">> => Message};
                error -> #{<<"message">> => Line}
            end;
        nomatch -> #{<<"message">> => Line}
    end.

file_error(Reason) -> unicode:characters_to_binary(file:format_error(Reason)).
