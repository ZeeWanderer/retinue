%%%-------------------------------------------------------------------
%% retinue_daemon OTP application.
%%%-------------------------------------------------------------------
-module(retinue_daemon_app).

-behaviour(application).

-export([start/2, stop/1]).

start(_StartType, _StartArgs) ->
    StateDir = retinue_paths:root(state),
    case {file:get_cwd(), retinue_paths:ensure_private(StateDir)} of
        {{ok, PreviousDir}, ok} ->
            case file:set_cwd(StateDir) of
                ok -> start_supervisor(PreviousDir);
                {error, Reason} -> {error, {state_directory_unusable, StateDir, Reason}}
            end;
        {{error, Reason}, _} -> {error, {working_directory_unavailable, Reason}};
        {_, {error, Reason}} -> {error, {state_directory_unusable, StateDir, Reason}}
    end.

start_supervisor(PreviousDir) ->
    retinue_incident_log:install(),
    case retinue_sup:start_link() of
        {ok, Pid} -> {ok, Pid, PreviousDir};
        Error ->
            _ = file:set_cwd(PreviousDir),
            Error
    end.

stop(PreviousDir) ->
    _ = logger:remove_handler(retinue_incidents),
    _ = file:set_cwd(PreviousDir),
    ok.
