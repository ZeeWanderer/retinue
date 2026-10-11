%%%-------------------------------------------------------------------
%% Shared query lexer and expression parser.
%%%-------------------------------------------------------------------
-module(wf_query_parse).

-export([parse/1, parse_arguments/1, and_all/1, extract_control/2, constraint_values/3,
         parse_op/1, split_vals/1]).

-type op() :: eq | neq | contains | gt | gte | lt | lte | default.
-type ast() :: match_all |
               {term, string()} |
               {filter, string(), op(), [string()]} |
               {'not', ast()} |
               {'and', [ast()]} |
               {'or', [ast()]}.

-doc "Parse free text, field filters, quotes, parentheses, and boolean operators into an AST.".
-spec parse(string() | binary()) -> {ok, ast()} | {error, string()}.
parse(Query0) ->
    Query = retinue_text:to_list(Query0),
    case lex(Query) of
        {ok, []} -> {ok, match_all};
        {ok, Tokens} ->
            case parse_or(Tokens) of
                {ok, Ast, []} -> {ok, Ast};
                {ok, _Ast, [rparen | _]} -> {error, "unexpected ')'"};
                {ok, _Ast, _Rest} -> {error, "unexpected query token"};
                Error -> Error
            end;
        Error -> Error
    end.

-doc "Parse shell argument fragments while preserving legacy multiword phrase and filter arguments.".
-spec parse_arguments([string()]) -> {ok, ast()} | {error, string()}.
parse_arguments(Args) ->
    parse(string:join([normalize_argument(Arg) || Arg <- Args], " ")).

-doc "Combine clauses with AND, dropping empty clauses and flattening nested AND nodes.".
-spec and_all([ast()]) -> ast().
and_all(Asts) ->
    make_and([Ast || Ast <- Asts, Ast =/= match_all]).

-doc "Remove a control filter when it is a positive top-level AND clause.".
-spec extract_control(ast(), string() | atom()) ->
    {ok, ast(), [#{op := op(), vals := [string()]}]} | {error, string()}.
extract_control({'and', Children}, Key) ->
    extract_control_children(Children, Key, [], []);
extract_control({filter, FilterKey, Op, Vals}, Key) ->
    case key_equal(FilterKey, Key) of
        true -> {ok, match_all, [#{op => Op, vals => Vals}]};
        false -> {ok, {filter, FilterKey, Op, Vals}, []}
    end;
extract_control(Ast, Key) ->
    case contains_control(Ast, Key) of
        true -> {error, control_error(Key)};
        false -> {ok, Ast, []}
    end.

-doc "Find a safe finite value set implied by one field, for source-file pruning.".
-spec constraint_values(ast(), string() | atom(), op()) -> unconstrained | {values, [string()]}.
constraint_values({filter, FilterKey, Op, Vals}, Key, Op) ->
    case key_equal(FilterKey, Key) of
        true -> {values, Vals};
        false -> unconstrained
    end;
constraint_values({'and', Children}, Key, Op) ->
    Constraints = [C || Child <- Children,
                         C <- [constraint_values(Child, Key, Op)],
                         C =/= unconstrained],
    union_constraints(Constraints);
constraint_values({'or', Children}, Key, Op) ->
    Constraints = [constraint_values(Child, Key, Op) || Child <- Children],
    case lists:member(unconstrained, Constraints) of
        true -> unconstrained;
        false -> union_constraints(Constraints)
    end;
constraint_values(_Ast, _Key, _Op) ->
    unconstrained.

%% Compatibility helpers retained for query controls and watch code.
-spec parse_op(string()) -> {ok, string(), op(), string()} | error.
parse_op(Arg) ->
    case lex(retinue_text:to_list(Arg)) of
        {ok, [{clause, Units}]} ->
            case clause_ast(Units) of
                {ok, {filter, Key, Op, Vals}} ->
                    {ok, Key, Op, string:join(Vals, "|")};
                _ -> error
            end;
        _ -> error
    end.

-spec split_vals(string() | binary()) -> [string()].
split_vals(Val) ->
    [string:trim(V) || V <- string:split(retinue_text:to_list(Val), "|", all), V =/= ""].

normalize_argument(Arg0) ->
    Arg = retinue_text:to_list(Arg0),
    case has_expression_syntax(Arg) of
        true -> Arg;
        false ->
            Units = [{C, bare} || C <- Arg],
            case clause_ast(Units) of
                {ok, {filter, Key, Op, Values}} -> render_filter(Key, Op, Values);
                _ -> quote_if_needed(Arg)
            end
    end.

has_expression_syntax(Arg) ->
    re:run(Arg, "(^|[[:space:]])(AND|OR|NOT)([[:space:]]|$)|[()\"]",
           [{capture, none}]) =:= match.

render_filter(Key, Op, Values) ->
    Key ++ op_text(Op) ++ string:join([quote(Value) || Value <- Values], "|").

op_text(eq) -> "=";
op_text(neq) -> "!=";
op_text(contains) -> "~";
op_text(gt) -> ">";
op_text(gte) -> ">=";
op_text(lt) -> "<";
op_text(lte) -> "<=";
op_text(default) -> ":".

quote_if_needed(Value) ->
    case lists:any(fun(C) -> C =:= $  orelse C =:= $\t orelse C =:= $\n orelse C =:= $\r end,
                   Value) of
        true -> quote(Value);
        false -> Value
    end.

quote(Value) ->
    EscapedBackslash = string:replace(Value, "\\", "\\\\", all),
    EscapedQuote = string:replace(EscapedBackslash, "\"", "\\\"", all),
    [$" | lists:flatten(EscapedQuote)] ++ [$"].

%% Lexer keeps quote/escape context on each character. That lets quoted or
%% escaped operators remain literal without string-reparse heuristics.
lex(Query) ->
    lex(Query, false, false, [], []).

lex([], true, _Started, _Units, _Tokens) ->
    {error, "unterminated quoted string"};
lex([], false, Started, Units, Tokens) ->
    {ok, lists:reverse(flush_clause(Started, Units, Tokens))};
lex([$\\], _Quoted, _Started, _Units, _Tokens) ->
    {error, "dangling escape at end of query"};
lex([$\\, C | Rest], Quoted, _Started, Units, Tokens) ->
    lex(Rest, Quoted, true, [{C, escaped} | Units], Tokens);
lex([$" | Rest], Quoted, _Started, Units, Tokens) ->
    lex(Rest, not Quoted, true, Units, Tokens);
lex([C | Rest], false, Started, Units, Tokens)
  when C =:= $ ; C =:= $\t; C =:= $\n; C =:= $\r ->
    lex(Rest, false, false, [], flush_clause(Started, Units, Tokens));
lex([$( | Rest], false, Started, Units, Tokens) ->
    Tokens1 = flush_clause(Started, Units, Tokens),
    lex(Rest, false, false, [], [lparen | Tokens1]);
lex([$) | Rest], false, Started, Units, Tokens) ->
    Tokens1 = flush_clause(Started, Units, Tokens),
    lex(Rest, false, false, [], [rparen | Tokens1]);
lex([C | Rest], Quoted, _Started, Units, Tokens) ->
    Context = case Quoted of true -> quoted; false -> bare end,
    lex(Rest, Quoted, true, [{C, Context} | Units], Tokens).

flush_clause(false, _Units, Tokens) -> Tokens;
flush_clause(true, Units0, Tokens) ->
    Units = lists:reverse(Units0),
    [classify_clause(Units) | Tokens].

classify_clause(Units) ->
    case all_bare(Units) of
        true ->
            case units_text(Units) of
                "AND" -> op_and;
                "OR" -> op_or;
                "NOT" -> op_not;
                _ -> {clause, Units}
            end;
        false -> {clause, Units}
    end.

all_bare(Units) ->
    lists:all(fun({_C, Context}) -> Context =:= bare end, Units).

parse_or(Tokens) ->
    case parse_and(Tokens) of
        {ok, Left, Rest} -> parse_or_tail(Left, Rest);
        Error -> Error
    end.

parse_or_tail(Left, [op_or | Rest]) ->
    case parse_and(Rest) of
        {ok, Right, Rest1} -> parse_or_tail(make_or([Left, Right]), Rest1);
        {error, _} -> {error, "OR requires an expression on both sides"}
    end;
parse_or_tail(Left, Rest) ->
    {ok, Left, Rest}.

parse_and(Tokens) ->
    case parse_unary(Tokens) of
        {ok, Left, Rest} -> parse_and_tail(Left, Rest);
        Error -> Error
    end.

parse_and_tail(Left, [op_and | Rest]) ->
    case parse_unary(Rest) of
        {ok, Right, Rest1} -> parse_and_tail(make_and([Left, Right]), Rest1);
        {error, _} -> {error, "AND requires an expression on both sides"}
    end;
parse_and_tail(Left, [Next | _] = Rest) ->
    case starts_unary(Next) of
        true ->
            case parse_unary(Rest) of
                {ok, Right, Rest1} -> parse_and_tail(make_and([Left, Right]), Rest1);
                Error -> Error
            end;
        false -> {ok, Left, Rest}
    end;
parse_and_tail(Left, []) ->
    {ok, Left, []}.

starts_unary({clause, _}) -> true;
starts_unary(lparen) -> true;
starts_unary(op_not) -> true;
starts_unary(_) -> false.

parse_unary([op_not | Rest]) ->
    case parse_unary(Rest) of
        {ok, Ast, Rest1} -> {ok, {'not', Ast}, Rest1};
        {error, _} -> {error, "NOT requires an expression"}
    end;
parse_unary(Tokens) ->
    parse_primary(Tokens).

parse_primary([{clause, Units} | Rest]) ->
    case clause_ast(Units) of
        {ok, Ast} -> {ok, Ast, Rest};
        Error -> Error
    end;
parse_primary([lparen, rparen | _]) ->
    {error, "empty parenthesized expression"};
parse_primary([lparen | Rest]) ->
    case parse_or(Rest) of
        {ok, Ast, [rparen | Rest1]} -> {ok, Ast, Rest1};
        {ok, _Ast, _} -> {error, "missing ')'"};
        Error -> Error
    end;
parse_primary([rparen | _]) ->
    {error, "unexpected ')'"};
parse_primary([op_or | _]) ->
    {error, "unexpected OR"};
parse_primary([op_and | _]) ->
    {error, "unexpected AND"};
parse_primary([]) ->
    {error, "expected query expression"}.

clause_ast([]) ->
    {error, "empty quoted term"};
clause_ast(Units) ->
    case find_operator(Units, []) of
        none -> {ok, {term, units_text(Units)}};
        {KeyUnits, Op, ValueUnits} ->
            case valid_key(KeyUnits) of
                false -> {ok, {term, units_text(Units)}};
                true ->
                    case split_value_units(ValueUnits) of
                        {ok, Values} -> {ok, {filter, units_text(KeyUnits), Op, Values}};
                        Error -> Error
                    end
            end
    end.

find_operator([], _KeyRev) -> none;
find_operator([{C1, bare}, {C2, bare} | Rest], KeyRev) ->
    case two_char_op(C1, C2) of
        undefined -> find_single_operator([{C1, bare}, {C2, bare} | Rest], KeyRev);
        Op -> {lists:reverse(KeyRev), Op, Rest}
    end;
find_operator([Unit | Rest], KeyRev) ->
    find_single_operator([Unit | Rest], KeyRev).

find_single_operator([{C, bare} | Rest], KeyRev) ->
    case one_char_op(C) of
        undefined -> find_operator(Rest, [{C, bare} | KeyRev]);
        Op -> {lists:reverse(KeyRev), Op, Rest}
    end;
find_single_operator([Unit | Rest], KeyRev) ->
    find_operator(Rest, [Unit | KeyRev]);
find_single_operator([], _KeyRev) -> none.

two_char_op($!, $=) -> neq;
two_char_op($>, $=) -> gte;
two_char_op($<, $=) -> lte;
two_char_op(_, _) -> undefined.

one_char_op($=) -> eq;
one_char_op($~) -> contains;
one_char_op($>) -> gt;
one_char_op($<) -> lt;
one_char_op($:) -> default;
one_char_op(_) -> undefined.

valid_key([]) -> false;
valid_key([{First, bare} | Rest]) ->
    valid_key_first(First) andalso
        lists:all(fun({C, Context}) -> Context =:= bare andalso valid_key_char(C) end, Rest);
valid_key(_) -> false.

valid_key_first(C) ->
    is_integer(C, $a, $z) orelse is_integer(C, $A, $Z) orelse C =:= $_.

valid_key_char(C) ->
    valid_key_first(C) orelse is_integer(C, $0, $9) orelse
        C =:= $. orelse C =:= $-.

split_value_units([]) ->
    {error, "filter requires a value"};
split_value_units(Units) ->
    Parts = split_value_units(Units, [], []),
    Values = [units_text(Part) || Part <- Parts],
    case lists:any(fun(Value) -> Value =:= "" end, Values) of
        true -> {error, "filter contains an empty alternative"};
        false -> {ok, Values}
    end.

split_value_units([], CurrentRev, PartsRev) ->
    lists:reverse([lists:reverse(CurrentRev) | PartsRev]);
split_value_units([{$|, bare} | Rest], CurrentRev, PartsRev) ->
    split_value_units(Rest, [], [lists:reverse(CurrentRev) | PartsRev]);
split_value_units([Unit | Rest], CurrentRev, PartsRev) ->
    split_value_units(Rest, [Unit | CurrentRev], PartsRev).

units_text(Units) ->
    [C || {C, _Context} <- Units].

make_and([]) -> match_all;
make_and([Only]) -> Only;
make_and(Asts) -> {'and', flatten('and', Asts)}.

make_or([Only]) -> Only;
make_or(Asts) -> {'or', flatten('or', Asts)}.

flatten(Tag, Asts) ->
    lists:append([case Ast of {Tag, Children} -> Children; _ -> [Ast] end || Ast <- Asts]).

extract_control_children([], _Key, KeptRev, ControlsRev) ->
    {ok, make_and(lists:reverse(KeptRev)), lists:reverse(ControlsRev)};
extract_control_children([{filter, FilterKey, Op, Vals} = Ast | Rest], Key, KeptRev, ControlsRev) ->
    case key_equal(FilterKey, Key) of
        true -> extract_control_children(Rest, Key, KeptRev,
                                         [#{op => Op, vals => Vals} | ControlsRev]);
        false -> extract_control_children(Rest, Key, [Ast | KeptRev], ControlsRev)
    end;
extract_control_children([Ast | Rest], Key, KeptRev, ControlsRev) ->
    case contains_control(Ast, Key) of
        true -> {error, control_error(Key)};
        false -> extract_control_children(Rest, Key, [Ast | KeptRev], ControlsRev)
    end.

contains_control({filter, FilterKey, _Op, _Vals}, Key) -> key_equal(FilterKey, Key);
contains_control({'and', Children}, Key) -> lists:any(fun(Ast) -> contains_control(Ast, Key) end, Children);
contains_control({'or', Children}, Key) -> lists:any(fun(Ast) -> contains_control(Ast, Key) end, Children);
contains_control({'not', Ast}, Key) -> contains_control(Ast, Key);
contains_control(_, _) -> false.

control_error(Key) ->
    lists:flatten(io_lib:format("~s may only be a top-level AND clause", [key_text(Key)])).

key_equal(A, B) ->
    string:lowercase(key_text(A)) =:= string:lowercase(key_text(B)).

key_text(Key) when is_atom(Key) -> atom_to_list(Key);
key_text(Key) -> retinue_text:to_list(Key).

union_constraints([]) -> unconstrained;
union_constraints(Constraints) ->
    Values = lists:append([Vals || {values, Vals} <- Constraints]),
    {values, unique(Values, [])}.

unique([], Acc) -> lists:reverse(Acc);
unique([Value | Rest], Acc) ->
    case lists:member(Value, Acc) of
        true -> unique(Rest, Acc);
        false -> unique(Rest, [Value | Acc])
    end.
