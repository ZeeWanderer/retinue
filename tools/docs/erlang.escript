#!/usr/bin/env escript
%%! +S 2:2
-mode(compile).

main([Base, Output | Apps]) ->
    ok = code:add_paths(filelib:wildcard(filename:join([Base,"lib","*","ebin"]))),
    Modules = lists:append([[module(A, B) || B <- filelib:wildcard(
        filename:join([Base,"lib",A,"ebin","*.beam"]))] || A <- Apps]),
    ok = file:write_file(Output, jsone:encode(Modules)).

module(App, Beam) ->
    {ok,{M,[{abstract_code,{raw_abstract_v1,Forms}},{compile_info,Info}]}} =
        beam_lib:chunks(Beam,[abstract_code,compile_info]),
    {ok,{docs_v1,_,_,Fmt,Doc,_,Entries}} = code:get_doc(M),
    Attrs = maps:from_list([P || F <- Forms, P <- declaration(F)]),
    Documented = sets:from_list([{N,A} || {{function,N,A},_,_,D,_} <- Entries, D =/= hidden]),
    #{app=>bin(App), name=>M, source=>bin(proplists:get_value(source,Info)),
      private_functions=>[[bin(io_lib:format("~s/~B",[N,A])),erl_anno:line(Anno)]
                          || {function,Anno,N,A,_} <- Forms, not sets:is_element({N,A},Documented)],
      description=>body(Doc,Fmt), behaviours=>[V || {attribute,_,K,V} <- Forms,
                                               K =:= behaviour orelse K =:= behavior],
      members=>[#{kind=>K,name=>N,arity=>A,line=>erl_anno:line(Anno),
                  description=>body(D,Fmt),signature=>maps:get({K,N,A},Attrs,bin(lists:join(<<"\n">>,Sig)))}
                || {{K,N,A},Anno,Sig,D,_} <- Entries, D =/= hidden]}.

declaration({attribute,_,Kind,{{N,A},_}}=F) when Kind=:=spec; Kind=:=callback ->
    K = case Kind of spec -> function; callback -> callback end,
    [{{K,N,A},bin(string:trim(erl_pp:attribute(F)))}];
declaration({attribute,_,Kind,{N,_,Vars}}=F) when Kind=:=type; Kind=:=opaque; Kind=:=nominal ->
    [{{type,N,length(Vars)},bin(string:trim(erl_pp:attribute(F)))}];
declaration(_) -> [].

body(#{<<"en">> := S},<<"text/markdown">>) -> S;
body(#{<<"en">> := S},_) -> bin([plain(N) || N <- S]);
body(_,_) -> <<>>.
plain(T) when is_binary(T) -> T;
plain({Tag,_,Nodes}) ->
    Text=[plain(N) || N <- Nodes],
    case Tag of p -> [Text,"\n\n"]; code -> ["`",Text,"`"]; _ -> Text end.
bin(S) -> unicode:characters_to_binary(S).
