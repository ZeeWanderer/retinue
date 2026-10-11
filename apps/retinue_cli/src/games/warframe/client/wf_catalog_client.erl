%%%-------------------------------------------------------------------
%% Thin CLI adapter for daemon-owned catalog queries.
%%%-------------------------------------------------------------------
-module(wf_catalog_client).

-export([query/2]).

-doc "Queue one typed catalog request and normalize the data-only daemon reply.".
-spec query(string(), map()) -> {ok, map(), map()} | {error, [iodata()]}.
query(Command, Query) ->
    Request = #{source => exports, command => Command, query => Query,
                cwd => filename:absname(".")},
    decode_reply(retinue_client:one_shot(warframe, Request)).

decode_reply({ok, #{query := Prepared, results := Results}}) ->
    {ok, Prepared, Results};
decode_reply({error, {query_errors, Errors}}) ->
    {error, Errors};
decode_reply({error, Reason}) ->
    {error, [retinue_client:format_error(Reason)]}.
