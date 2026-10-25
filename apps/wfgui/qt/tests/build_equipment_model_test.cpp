#include <QJsonArray>
#include <QJsonObject>
#include <QtTest>

#include "build_equipment_model.h"

class BuildEquipmentModelTest final : public QObject {
  Q_OBJECT

private slots:
  void groupsConcreteInstances();
  void filtersLocally();
  void rejectsOrphanInstances();
};

namespace {
QJsonObject equipment() {
  return {
      {"player_revision", 8},
      {"player_updated_at", 1234},
      {"definitions",
       QJsonArray{
           QJsonObject{{"id", "/Lotus/Powersuits/Excalibur/DoomSword"},
                       {"name", "Exalted Blade"},
                       {"class", "exalted"},
                       {"asset", QJsonObject{{"id", "exalted-blade"}}}},
           QJsonObject{{"id", "/Lotus/Weapons/Tenno/Rifle"},
                       {"name", "Test Rifle"},
                       {"class", "primary"}},
       }},
      {"instances",
       QJsonArray{
           QJsonObject{{"instance_id", "exalted-2"},
                       {"definition_id",
                        "/Lotus/Powersuits/Excalibur/DoomSword"},
                       {"custom_name", "Second Blade"}},
           QJsonObject{{"instance_id", "exalted-1"},
                       {"definition_id",
                        "/Lotus/Powersuits/Excalibur/DoomSword"}},
           QJsonObject{{"instance_id", "rifle-1"},
                       {"definition_id", "/Lotus/Weapons/Tenno/Rifle"}},
       }},
  };
}
} // namespace

void BuildEquipmentModelTest::groupsConcreteInstances() {
  BuildEquipmentModel model;
  QString error;
  QVERIFY(model.replace(equipment(), &error));
  QCOMPARE(error, QString());
  QCOMPARE(model.revision(), qint64(8));
  QCOMPARE(model.updatedAt(), qint64(1234));
  QCOMPARE(model.rowCount(), 2);

  const QModelIndex exalted = model.index(0);
  QCOMPARE(exalted.data(BuildEquipmentModel::NameRole).toString(),
           QString("Exalted Blade"));
  QCOMPARE(exalted.data(BuildEquipmentModel::ClassRole).toString(),
           QString("exalted"));
  QCOMPARE(exalted.data(BuildEquipmentModel::InstanceCountRole).toInt(), 2);
}

void BuildEquipmentModelTest::filtersLocally() {
  BuildEquipmentModel model;
  QVERIFY(model.replace(equipment()));
  BuildEquipmentFilterModel filter;
  filter.setSourceModel(&model);

  filter.setCategory("primary");
  QCOMPARE(filter.rowCount(), 1);
  QCOMPARE(filter.index(0, 0).data(BuildEquipmentModel::NameRole).toString(),
           QString("Test Rifle"));

  filter.setCategory("all");
  filter.setText("Second Blade");
  QCOMPARE(filter.rowCount(), 1);
  QCOMPARE(filter.index(0, 0).data(BuildEquipmentModel::NameRole).toString(),
           QString("Exalted Blade"));
}

void BuildEquipmentModelTest::rejectsOrphanInstances() {
  BuildEquipmentModel model;
  QJsonObject malformed = equipment();
  malformed.insert(
      "instances",
      QJsonArray{QJsonObject{{"instance_id", "missing"},
                             {"definition_id", "/missing"}}});
  QString error;
  QVERIFY(!model.replace(malformed, &error));
  QVERIFY(!error.isEmpty());
}

QTEST_MAIN(BuildEquipmentModelTest)

#include "build_equipment_model_test.moc"
