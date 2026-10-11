%%%-------------------------------------------------------------------
%% Thin adapter from wfcli tree nodes to dirtree terminal output.
%%%-------------------------------------------------------------------
-module(retinue_tree).

-export([format/1]).

-type tree_node() :: {unicode:chardata(), [tree_node()]}.

-doc "Render a labeled tree as UTF-8 terminal lines.".
-spec format(tree_node()) -> iodata().
format(Tree) ->
    {Encoded, Labels, _Next} = encode_tree(Tree, 0),
    Lines = dirtree:pretty_print(Encoded, 1),
    [[restore_label(Line, Labels), $\n] || Line <- Lines].

encode_tree({Label, Children}, Index) ->
    Token = iolist_to_binary(
              ["WFCLI_TREE_NODE_", integer_to_list(Index), "_END"]),
    {EncodedChildren, Labels, Next} = encode_children(Children, Index + 1),
    {{dirpath, Token, EncodedChildren},
     [{Token, unicode:characters_to_binary(Label)} | Labels], Next}.

encode_children([], Index) -> {[], [], Index};
encode_children([Child | Rest], Index) ->
    {EncodedChild, ChildLabels, Next} = encode_tree(Child, Index),
    {EncodedRest, RestLabels, Last} = encode_children(Rest, Next),
    {[EncodedChild | EncodedRest], ChildLabels ++ RestLabels, Last}.

restore_label(Line, [{Token, Label} | Rest]) ->
    case binary:match(Line, Token) of
        nomatch -> restore_label(Line, Rest);
        _ -> binary:replace(Line, Token, Label)
    end;
restore_label(Line, []) -> Line.
