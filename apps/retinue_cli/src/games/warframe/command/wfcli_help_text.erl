-module(wfcli_help_text).

-export([query_guide/0, query_examples/0, query_examples/1]).

query_guide() ->
    [
        "  - Adjacent terms are ANDed. Use uppercase OR, AND, NOT, and parentheses.\n",
        "  - Precedence: NOT, then AND, then OR. Quoted text matches one substring phrase.\n",
        "  - Operators: =, !=, ~, >=, <=, >, <, or key:value for defaults.\n",
        "  - Inside one filter, value1|value2 means either value; use OR between expressions.\n",
        "  - Backslash escapes the next syntax character. Boolean keywords are uppercase only.\n",
        "  - Sorting: sort=field or sort=-field (desc).\n"
    ].

query_examples() -> query_examples("wfcli").

query_examples(Name) ->
    [
        "  query: ", Name, " query \"type=Fissure void\"\n",
        "  query: ", Name, " query \"type=Fissure|Alert data.MissionType=MT_DEFENSE\"\n",
        "  query: ", Name, " query '(fissure OR alert) NOT expired'\n",
        "  query: ", Name, " query \"type=Alert sort=expiry\"\n",
        "  query: ", Name, " watch --spec \"alerts:reward~endo\"\n",
        "  query: ", Name, " watch --spec \"fissures:extract=data.Node\"\n"
    ].
