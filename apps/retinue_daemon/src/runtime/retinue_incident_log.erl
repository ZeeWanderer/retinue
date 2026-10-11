-module(retinue_incident_log).

-export([install/0, path/0, format/2]).

path() ->
    application:get_env(retinue_daemon, incident_log_file, retinue_paths:file(state, "daemon.log")).

install() ->
    case logger:get_handler_config(retinue_incidents) of
        {ok, _} -> logger:set_handler_config(retinue_incidents, formatter, {?MODULE, #{}});
        {error, _} -> add_handler()
    end.

add_handler() ->
    Path = path(),
    Config = #{level => warning,
               config => #{file => Path, max_no_bytes => 1024 * 1024, max_no_files => 2},
               formatter => {?MODULE, #{}}},
    Result = case filelib:ensure_dir(Path) of
        ok -> logger:add_handler(retinue_incidents, logger_std_h, Config);
        Error -> Error
    end,
    case Result of
        ok -> ok;
        {error, {already_exist, retinue_incidents}} -> ok;
        {error, Reason} ->
            logger:error("daemon incident log unavailable: ~ts: ~p", [Path, Reason])
    end.

format(#{level := Level, meta := Meta} = Event, _Config) ->
    Message = logger_formatter:format(Event, #{template => [msg], single_line => true, max_size => 8192}),
    Millis = maps:get(time, Meta, erlang:system_time(microsecond)) div 1000,
    [json:encode(#{<<"timestamp_ms">> => Millis,
                   <<"level">> => case Level of warning -> <<"warn">>; _ -> atom_to_binary(Level) end,
                   <<"event">> => <<"daemon.log">>,
                   <<"message">> => unicode:characters_to_binary(string:trim(Message))}), $\n].
