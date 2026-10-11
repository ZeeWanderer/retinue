%%%-------------------------------------------------------------------
%% Shared time formatting helpers.
%%%-------------------------------------------------------------------
-module(retinue_time).

-export([format_millis/1, format_millis/2, parse/1, parse/2]).

format_millis(Ms) ->
    format_millis(Ms, #{}).

format_millis(Ms, Opts) when is_integer(Ms) ->
    Utc = maps:get(utc, Opts, false)
          orelse maps:get(output_format, Opts, table) =:= json,
    Offset = case Utc of true -> "Z"; false -> "" end,
    Unit = maps:get(precision, Opts, second),
    Time = erlang:convert_time_unit(Ms, millisecond, Unit),
    try calendar:system_time_to_rfc3339(Time, [{unit, Unit}, {offset, Offset}])
    catch error:_ -> "unknown" end;
format_millis(Value, _Opts) when Value =:= undefined; Value =:= null -> "unknown";
format_millis(Value, Opts) ->
    Str = retinue_text:to_list(Value),
    case parse(Str) of
        {ok, Millis} -> format_millis(Millis, Opts);
        error -> Str
    end.

parse(Value) -> parse(Value, erlang:system_time(millisecond)).

parse(Value, _Now) when is_integer(Value) -> {ok, Value};
parse(Value, Now) when is_binary(Value) -> parse(binary_to_list(Value), Now);
parse(Value, Now) when is_list(Value) ->
    Text = string:lowercase(string:trim(Value)),
    case Text of
        "now" -> {ok, Now};
        "now" ++ Rest -> relative(Rest, Now);
        _ ->
            case string:to_integer(Text) of
                {Millis, ""} -> {ok, Millis};
                _ -> parse_rfc3339(Text)
            end
    end;
parse(_, _) -> error.

relative(Text, Now) ->
    case re:run(Text, "^([+-])([0-9]+)(ms|s|m|h|d|w)$", [{capture, all_but_first, list}]) of
        {match, [Sign, Count, Unit]} ->
            Offset = list_to_integer(Count) * duration_unit(Unit),
            {ok, Now + case Sign of "+" -> Offset; "-" -> -Offset end};
        nomatch -> error
    end.

duration_unit("ms") -> 1;
duration_unit("s") -> 1000;
duration_unit("m") -> 60000;
duration_unit("h") -> 3600000;
duration_unit("d") -> 86400000;
duration_unit("w") -> 604800000.

parse_rfc3339(Text) ->
    Pattern = "^([0-9]{4})-([0-9]{2})-([0-9]{2})[t ]"
              "(?:[01][0-9]|2[0-3]):[0-5][0-9]:(?:[0-5][0-9]|60)(?:\\.[0-9]+)?"
              "(?:z|[+-](?:[01][0-9]|2[0-3]):[0-5][0-9])$",
    try
        {match, Date} = re:run(Text, Pattern, [{capture, all_but_first, list}]),
        true = calendar:valid_date(list_to_tuple([list_to_integer(Part) || Part <- Date])),
        {ok, calendar:rfc3339_to_system_time(string:uppercase(Text), [{unit, millisecond}])}
    catch error:_ -> error end.
