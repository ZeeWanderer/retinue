-module(wfcli_stat_upgrades).

-export([ranged/1]).

ranged(<<"/Lotus/Upgrades/Mods/Rifle/", Key/binary>>) -> lookup(rifle, Key);
ranged(<<"/Lotus/Upgrades/Mods/Shotgun/", Key/binary>>) -> lookup(shotgun, Key);
ranged(<<"/Lotus/Upgrades/Mods/Pistol/", Key/binary>>) -> lookup(pistol, Key);
ranged(_) -> unsupported.

lookup(Family, Key) ->
    case spec(Family, Key) of
        unsupported -> unsupported;
        Rule -> Rule#{family => Family}
    end.

spec(_, <<"WeaponFireDamageMod">>) -> elemental(5, heat);
spec(_, <<"WeaponFreezeDamageMod">>) -> elemental(5, cold);
spec(_, <<"WeaponElectricityDamageMod">>) -> elemental(5, electricity);
spec(_, <<"WeaponToxinDamageMod">>) -> elemental(5, toxin);
spec(_, <<"WeaponStunChanceMod">>) -> scalar(5, status_chance, 0.15000001);
spec(pistol, <<"WeaponDamageAmountMod">>) -> scalar(10, damage, 0.2);
spec(rifle, <<"WeaponDamageAmountMod">>) -> scalar(10, damage, 0.15000001);
spec(shotgun, <<"WeaponDamageAmountMod">>) -> scalar(5, damage, 0.15000001);
spec(rifle, <<"WeaponCritChanceMod">>) -> scalar(5, critical_chance, 0.25);
spec(pistol, <<"WeaponCritChanceMod">>) -> scalar(5, critical_chance, 0.2);
spec(shotgun, <<"WeaponCritChanceMod">>) -> scalar(5, critical_chance, 0.15000001);
spec(rifle, <<"WeaponCritDamageMod">>) -> scalar(5, critical_multiplier, 0.2);
spec(_, <<"WeaponCritDamageMod">>) -> scalar(5, critical_multiplier, 0.1);
spec(rifle, <<"WeaponFireIterationsMod">>) -> scalar(5, multishot, 0.15000001);
spec(_, <<"WeaponFireIterationsMod">>) -> scalar(5, multishot, 0.2);
spec(rifle, <<"WeaponFireRateMod">>) -> scalar(5, fire_rate, 0.1);
spec(pistol, <<"WeaponFireRateMod">>) -> scalar(5, fire_rate, 0.12);
spec(shotgun, <<"WeaponFireRateMod">>) -> scalar(5, fire_rate, 0.15000001);
spec(rifle, <<"WeaponReloadSpeedMod">>) -> scalar(5, reload_speed, 0.050000001);
spec(pistol, <<"WeaponReloadSpeedMod">>) -> scalar(5, reload_speed, 0.079999998);
spec(shotgun, <<"WeaponReloadSpeedMod">>) -> scalar(5, reload_speed, 0.1);
spec(shotgun, <<"WeaponClipMaxMod">>) -> scalar(5, magazine, 0.1);
spec(_, <<"WeaponClipMaxMod">>) -> scalar(5, magazine, 0.050000001);
spec(pistol, <<"Expert/WeaponCritChanceModBeginnerExpert">>) -> scalar(10, critical_chance, 0.17);
spec(pistol, <<"Expert/PrimedWeaponCritDamageMod">>) -> scalar(10, critical_multiplier, 0.1);
spec(pistol, <<"Expert/PrimedWeaponElectricityDamageMod">>) -> elemental(10, electricity);
spec(pistol, <<"Expert/WeaponFireDamageModExpert">>) -> elemental(10, heat);
spec(shotgun, <<"Expert/WeaponDamageAmountModExpert">>) -> scalar(10, damage, 0.15000001);
spec(shotgun, <<"Expert/WeaponElectricityDamageModExpert">>) -> elemental(10, electricity);
spec(shotgun, <<"Expert/WeaponFreezeDamageModExpert">>) -> elemental(10, cold);
spec(rifle, <<"DualStat/PoisonEventRifleMod">>) -> status_element(toxin);
spec(pistol, <<"DualStat/PoisonEventPistolMod">>) -> status_element(toxin);
spec(shotgun, <<"DualStat/PoisonEventShotgunMod">>) -> status_element(toxin);
spec(rifle, <<"DualStat/RadiationReloadRifleMod">>) ->
    rule(3, [{{element, radiation}, add, 0.15000001}, {reload_speed, stacking_multiply, 0.1}]);
spec(shotgun, <<"DualStat/MagneticFireRateShotgunMod">>) ->
    rule(3, [{{element, magnetic}, add, 0.15000001}, {fire_rate, stacking_multiply, 0.1}]);
spec(rifle, <<"DualStat/CorruptedCritRateFireRateRifle">>) -> crit_rate();
spec(shotgun, <<"DualStat/CorruptedCritChanceFireRateShotgun">>) -> crit_rate();
spec(pistol, <<"DualStat/CorruptedFireRateDamagePistol">>) ->
    rule(5, [{fire_rate, stacking_multiply, 0.15000001}, {damage, stacking_multiply, -0.025}]);
spec(pistol, <<"DualStat/GrinderMod">>) ->
    rule(5, [{fire_rate, stacking_multiply, 0.1}, {multishot, stacking_multiply, 0.1}]);
spec(pistol, <<"PistolStatusDamageDuoMod">>) ->
    rule(5, [{status_damage, stacking_multiply, 0.15000001}, {reload_speed, stacking_multiply, 0.1}]);
spec(shotgun, <<"ShotgunStatusDamageDuoMod">>) ->
    rule(5, [{status_damage, stacking_multiply, 0.15000001}, {magazine, stacking_multiply, 0.1}]);
spec(rifle, <<"WeaponFireIterationsSPMod">>) ->
    multishot(0.072729997, 0.02727, 5, <<"galvanized-chamber">>);
spec(shotgun, <<"WeaponFireIterationsSPMod">>) ->
    multishot(0.1, 0.02727, 4, <<"galvanized-hell">>);
spec(pistol, <<"WeaponFireIterationsSPMod">>) ->
    multishot(0.1, 0.027272699, 4, <<"galvanized-diffusion">>);
spec(pistol, <<"WeaponStatusChanceSPMod">>) -> galvanized_status(0.072727002);
spec(rifle, <<"WeaponStatusChanceSPMod">>) -> galvanized_status(0.072727002);
spec(shotgun, <<"WeaponStatusChanceSPMod">>) -> galvanized_status(0.072729997);
spec(pistol, <<"WeaponRecoilReductionMod">>) -> recoil();
spec(shotgun, <<"WeaponRecoilReductionMod">>) -> recoil();
spec(rifle, <<"WeaponNoiseReductionMod">>) ->
    (rule(3, []))#{outside_scope => <<"Noise does not change an ideal on-target hit">>};
spec(_, _) -> unsupported.

rule(MaxRank, Effects) -> #{max_rank => MaxRank, effects => Effects}.
scalar(MaxRank, Stat, Value) -> rule(MaxRank, [{Stat, stacking_multiply, Value}]).
elemental(MaxRank, Element) -> rule(MaxRank, [{{element, Element}, add, 0.15000001}]).
status_element(Element) ->
    rule(3, [{{element, Element}, add, 0.15000001}, {status_chance, stacking_multiply, 0.15000001}]).
crit_rate() ->
    rule(5, [{critical_chance, stacking_multiply, 0.33333001},
             {fire_rate, stacking_multiply, -0.033300001}]).
multishot(Base, Step, Cap, Id) ->
    (scalar(10, multishot, Base))#{conditional =>
        #{id => Id, max_stacks => Cap, duration => 20, activation => <<"On kill">>,
          effects => [{multishot, stacking_multiply, Step}]}}.
galvanized_status(Value) ->
    (scalar(10, status_chance, Value))#{
        excluded => [<<"Galvanized per-status conditional damage is not evaluated">>]}.
recoil() ->
    (rule(3, []))#{outside_scope => <<"Recoil does not change an ideal on-target hit">>}.
