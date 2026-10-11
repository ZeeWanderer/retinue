-module(wf_stat_rules).

-export([provenance/0, frame/1, upgrade/1, melee/1, shard/1, evolutions/1]).

provenance() ->
    #{revision => 11,
      rules_hash => binary:encode_hex(?MODULE:module_info(md5), lowercase),
      evaluator_hash => binary:encode_hex(wf_stat_math:module_info(md5), lowercase),
      effects_hash => binary:encode_hex(wf_stat_effects:module_info(md5), lowercase),
      buildset_hash => binary:encode_hex(wf_stat_buildset:module_info(md5), lowercase),
      profiles_hash => binary:encode_hex(wf_stat_profiles:module_info(md5), lowercase),
      upgrades_hash => binary:encode_hex(wf_stat_upgrades:module_info(md5), lowercase),
      frames_hash => binary:encode_hex(wf_stat_frames:module_info(md5), lowercase),
      abilities_hash => binary:encode_hex(wf_stat_abilities:module_info(md5), lowercase),
      executable_sha256 =>
          <<"45fa6ad0769cc8ca7fa7e0ffdee65c0c0932e11744146781ad18c45b16e4a81c">>,
      packages_sha256 =>
          <<"4e242f86102d9b075c0804eda5f59e51da2056890aa67c7241da4ff4555f3270">>,
      validation => <<"static research; live damage parity not yet verified">>}.

frame(Type) when Type =:= <<"/Lotus/Powersuits/Cowgirl/MesaPrime">>;
                 Type =:= <<"/Lotus/Powersuits/Cowgirl/Cowgirl">> ->
    #{mechanic => <<"calculator.mesa">>,
      passive => [#{stat => reload_speed, operation => stacking_multiply,
                    value => 0.25, requires => single_sidearm},
                  #{stat => fire_rate, operation => stacking_multiply,
                    value => 0.15, requires => dual_sidearm}],
      no_melee_health => 50};
frame(_) -> unsupported.

melee(<<"/Lotus/Weapons/Tenno/Melee/Dagger/CeramicDagger">>) ->
    #{initial_combo => 0, combo_step => 20, combo_cap => 220, recovery_per_second => 40};
melee(_) -> unsupported.

upgrade(<<"/Lotus/Upgrades/Mods/Sets/Umbra/WarframeUmbraModC">>) ->
    (rule(10, [{ability_strength, stacking_multiply, 0.039999999}]))#{
        source_class => <<"warframe">>,
        excluded => [<<"Umbral set amplification and Tau resistance are not evaluated">>]};
upgrade(<<"/Lotus/Upgrades/Mods/Warframe/DualStat/ConstitutionMod">>) ->
    (rule(3, [{ability_duration, stacking_multiply, 0.07}]))#{
        source_class => <<"warframe">>,
        outside_scope => <<"Knockdown recovery is outside direct-fire estimates">>};
upgrade(<<"/Lotus/Upgrades/Mods/Warframe/AvatarInvulnOnRollMod">>) ->
    (rule(10, []))#{outside_scope => <<"Invulnerability is outside direct-fire estimates">>};
upgrade(<<"/Lotus/Upgrades/Mods/Warframe/AvatarPickupBonusMod">>) ->
    (rule(10, []))#{outside_scope => <<"Pickup conversion is outside direct-fire estimates">>};
upgrade(<<"/Lotus/Upgrades/Mods/Aura/PlayerEnergyHealthRegenAuraMod">>) ->
    (rule(5, []))#{outside_scope => <<"Regeneration is outside direct-fire estimates">>};
upgrade(<<"/Lotus/Powersuits/Cowgirl/GunFuPvPAugmentCard">>) ->
    (rule(3, []))#{outside_scope => <<"Peacemaker movement is outside Vasto direct-fire estimates">>};
upgrade(<<"/Lotus/Upgrades/CosmeticEnhancers/Offensive/GolemArcanePistolSpeedOnCrit">>) ->
    (rule(5, []))#{target => <<"secondary">>, conditional =>
        #{id => <<"arcane-velocity">>, max_stacks => 1, duration => 9,
          effects => [{fire_rate, stacking_multiply, 0.2}]}};
upgrade(<<"/Lotus/Upgrades/CosmeticEnhancers/Offensive/AbilityHeatProcsGiveCritChance">>) ->
    (rule(5, []))#{target => <<"secondary">>, conditional =>
        #{id => <<"arcane-hot-shot">>, max_stacks => 50, duration => 10,
          effects => [{critical_chance, stacking_multiply, 0.0099999998}]}};
upgrade(<<"/Lotus/Upgrades/CosmeticEnhancers/Offensive/StatusAndAmmoEfficiencyOnWeakpointHit">>) ->
    (rule(5, []))#{mechanic => <<"calculator.primary-crux">>, source_class => <<"primary">>, conditional =>
        #{id => <<"primary-crux">>, max_stacks => 10, duration => 10,
          activation => <<"Weakpoint hits; normal-mode ammo only">>,
          effects => [{status_chance, stacking_multiply, 0.050000001},
                      {ammo_consumption, stacking_multiply, -0.0099999998}]}};
upgrade(<<"/Lotus/Upgrades/CosmeticEnhancers/Offensive/MultishotForCombo">>) ->
    (rule(5, []))#{mechanic => <<"calculator.secondary-outburst">>, derive => outburst,
        values_by_rank => [0.05, 0.07, 0.09, 0.12, 0.16, 0.2],
        conditional => #{id => <<"secondary-outburst">>, max_stacks => 1, lower_stacks => 1, duration => 30,
                         activation => <<"Switch to secondary; consumes melee combo">>, effects => []}};
upgrade(<<"/Lotus/Upgrades/Mods/Melee/DualStat/CorruptedHeavyDamageChargeSpeedMod">>) ->
    (rule(3, [{initial_combo, add, 7.5}]))#{
        outside_scope => <<"Combo duration affects buildup, not the recovered combo floor">>};
upgrade(<<"/Lotus/Upgrades/Mods/Melee/WeaponMeleeStealthLethalMod">>) ->
    (rule(3, [{initial_combo, add, 4}]))#{outside_scope => <<"Melee finisher damage is not evaluated">>};
upgrade(<<"/Lotus/Upgrades/CosmeticEnhancers/Offensive/PermaComboOnFinisher">>) ->
    (rule(5, []))#{mechanic => <<"calculator.combo">>, conditional =>
        #{id => <<"melee-crescendo">>, max_stacks => 240, rank_scaled_cap => true,
          activation => <<"Finisher kills; persists for the mission">>,
          effects => [{initial_combo, add, 1}]}};
upgrade(Type) ->
    case wf_stat_frames:upgrade(Type) of
        unsupported -> wf_stat_upgrades:ranged(Type);
        Rule -> Rule
    end.

rule(MaxRank, Effects) -> #{max_rank => MaxRank, effects => Effects}.

shard(<<"/Lotus/Upgrades/Invigorations/ArchonCrystalUpgrades/ArchonCrystalUpgradeWeaponCritBoostFromHeat">>) ->
    topaz(0.01);
shard(<<"/Lotus/Upgrades/Invigorations/ArchonCrystalUpgrades/ArchonCrystalUpgradeWeaponCritBoostFromHeatMythic">>) ->
    topaz(0.015);
shard(_) -> unsupported.

topaz(Step) ->
    (rule(0, []))#{mechanic => <<"calculator.topaz">>, target => <<"secondary">>, conditional =>
        #{id => <<"topaz-heat-kills">>, max_stacks => 50,
          activation => <<"Kill enemies affected by Heat status">>,
          effects => [{critical_chance, stacking_multiply, Step}]}}.

evolutions(<<"/Lotus/Weapons/Tenno/Pistols/PrimeVasto/PrimeVastoPistol">>) ->
    Dir = <<"/Lotus/Weapons/Tenno/Pistol/VastoIncarnonEvolutions/">>,
    [[perk(<<Dir/binary, "VastoIncarnonAltFire">>, <<"Incarnon form">>, [])],
     [(perk(<<Dir/binary, "Prime/PrimeVastoEvoTier1A">>, <<"Lone Gun">>,
            [{base_damage, add_base, 24}]))#{conditional =>
          #{id => <<"lone-gun">>, max_stacks => 1, automatic => true,
            requires_absent => <<"primary">>,
            activation => <<"No primary equipped">>,
            effects => [{base_damage, add_base, 40}, {magazine, add_base, 14}]}},
      (perk(<<Dir/binary, "Prime/PrimeVastoEvoTier1B">>, <<"Deathtrap Trigger">>,
            [{base_damage, add_base, 24}]))#{conditional =>
          #{id => <<"deathtrap-trigger">>, max_stacks => 1, duration => 3,
            requires_class => <<"primary">>, activation => <<"Switch from primary">>,
            effects => [{critical_chance, add_base, 0.30000001},
                        {critical_multiplier, add_base, 0.80000001}]}}],
     [perk(<<Dir/binary, "Prime/PrimeVastoEvoTier2A">>, <<"Extended Volley">>,
           [{magazine, add_base, 6}]),
      (perk(<<Dir/binary, "VastoEvoTier2B">>, <<"Marksman's Hand">>, []))#{
          outside_scope => <<"Recoil does not change an ideal on-target hit">>},
      (perk(<<Dir/binary, "VastoEvoTier2C">>, <<"Awakened Readiness">>, []))#{
          outside_scope => <<"Holstered reload is outside uninterrupted firing">>}],
     [perk(<<Dir/binary, "Prime/PrimeVastoEvoTier3A">>, <<"Commodore's Fortune">>,
           [{critical_chance, add_base, 0.18000001}]),
      perk(<<Dir/binary, "Prime/PrimeVastoEvoTier3B">>, <<"Survivor's Edge">>,
           [{critical_chance, add_base, 0.1}, {status_chance, add_base, 0.1}]),
      (perk(<<Dir/binary, "Prime/PrimeVastoEvoTier3C">>, <<"Reaper's Plenty">>, []))#{
          conditional => #{id => <<"reapers-plenty">>, max_stacks => 1, duration => 2,
                           activation => <<"Headshot; normal-mode ammo only">>,
                           effects => [{ammo_consumption, stacking_multiply, -1}]}}]];
evolutions(Type) when Type =:= <<"/Lotus/Weapons/Tenno/Shotgun/FullAutoShotgun">>;
                      Type =:= <<"/Lotus/Weapons/Tenno/Shotgun/PrimeBoar">> ->
    Dir = <<"/Lotus/Weapons/Tenno/Shotgun/BoarIncarnonEvolutions/">>,
    [[perk(<<Dir/binary, "BoarIncarnonAltFire">>, <<"Incarnon form">>, [])],
     [(perk(<<Dir/binary, "BoarEvoTier1A">>, <<"Reload-from-empty damage">>,
            [{base_damage, add_base, 10}]))#{conditional =>
          #{id => <<"boar-empty-reload-damage">>, max_stacks => 1,
            activation => <<"Reload from empty; resets on a nonempty reload">>,
            effects => [{base_damage, add_base, 14}]}},
      (perk(<<Dir/binary, "BoarEvoTier1B">>, <<"Fortress Salvo">>,
            [{base_damage, add_base, 16}]))#{
          outside_scope => <<"Armor-gated punch through does not change one on-target hit">>}],
     [(perk(<<Dir/binary, "BoarEvoTier2A">>, <<"Reload-from-empty speed">>, []))#{conditional =>
          #{id => <<"boar-empty-reload-speed">>, max_stacks => 1,
            activation => <<"Reload from empty; resets on a nonempty reload">>,
            effects => [{reload_speed, stacking_multiply, 1}]}},
      (perk(<<Dir/binary, "BoarEvoTier2B">>, <<"Reserve ammunition">>, []))#{
          outside_scope => <<"Ammo reserve does not change ideal reload-adjusted DPS">>},
      (perk(<<Dir/binary, "BoarEvoTier2C">>, <<"Accuracy">>, []))#{
          outside_scope => <<"Spread does not change an ideal on-target hit">>}],
     [perk(<<Dir/binary, "BoarEvoTier3A">>, <<"Status chance">>,
           [{status_chance, add_base, 0.95999998}]),
      perk(<<Dir/binary, "BoarEvoTier3B">>, <<"Critical chance and damage">>,
           [{critical_chance, add_base, 0.2}, {critical_multiplier, add_base, 0.5}]),
      perk(<<Dir/binary, "BoarEvoTier3C">>, <<"Critical and status chance">>,
           [{critical_chance, add_base, 0.1}, {status_chance, add_base, 0.47999999}])]];
evolutions(<<"/Lotus/Weapons/Tenno/Melee/Dagger/CeramicDagger">>) ->
    Dir = <<"/Lotus/Weapons/Tenno/Melee/Dagger/CeramicDaggerIncarnonEvolutions/">>,
    Outside = <<"Melee attack damage is outside this target's direct-fire estimate">>,
    [[perk(<<Dir/binary, "CeramicDaggerEvoTransform">>, <<"Incarnon form">>, [])],
     [(perk(<<Dir/binary, "CeramicDaggerEvoTier1A">>, <<"Gun and Blade">>, []))#{
          outside_scope => Outside, conditional =>
              #{id => <<"gun-and-blade">>, max_stacks => 100, requires_class => <<"primary">>,
                activation => <<"Primary weapon kills; persists for the mission">>,
                effects => [{initial_combo, add, 1}]}},
      (perk(<<Dir/binary, "CeramicDaggerEvoTier1B">>, <<"Ceramic evolution II/B">>, []))#{
          excluded => [<<"Ceramic evolution II/B effects are not reviewed">>]}],
     [(perk(<<Dir/binary, "CeramicDaggerEvoTier2A">>, <<"Origin Reach">>, []))#{
          outside_scope => Outside},
      perk(<<Dir/binary, "CeramicDaggerEvoTier2B">>, <<"Adept Reflexes">>,
           [{initial_combo, add, 20}]),
      (perk(<<Dir/binary, "CeramicDaggerEvoTier2C">>, <<"Melee-kill firearm reload">>, []))#{
          excluded => [<<"Melee-kill firearm reload is not evaluated">>]}],
     [(perk(<<Dir/binary, "CeramicDaggerEvoTier3A">>, <<"First-attack critical damage">>, []))#{
          outside_scope => Outside},
      (perk(<<Dir/binary, "CeramicDaggerEvoTier3B">>, <<"Status chance">>, []))#{outside_scope => Outside},
      (perk(<<Dir/binary, "CeramicDaggerEvoTier3C">>, <<"Critical chance">>, []))#{outside_scope => Outside}]];
evolutions(_) -> unsupported.

perk(Type, Name, Effects) ->
    (rule(0, Effects))#{mechanic => <<"calculator.evolutions">>, item_type => Type, item => Name}.
