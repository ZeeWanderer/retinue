#include <QJsonArray>
#include <QJsonObject>
#include <QtTest>

#include "build_source_model.h"

class BuildSourceModelTest final : public QObject {
  Q_OBJECT

private slots:
  void exposesItems();
  void exposesBuildSummaries();
  void rejectsMalformedResponses();
};

void BuildSourceModelTest::exposesItems() {
  BuildItemModel model;
  const QJsonObject response{
      {"items",
       QJsonArray{QJsonObject{{"external_id", 5952},
                              {"canonical_id", "/Lotus/RevenantPrime"},
                              {"name", "Revenant Prime"},
                              {"class", "warframe"},
                              {"categories", QJsonArray{"warframe"}},
                              {"texture", "revenant.png"}}}}};
  QString error;
  QVERIFY(model.replace(response, &error));
  QCOMPARE(error, QString());
  QCOMPARE(model.rowCount(), 1);
  const QModelIndex item = model.index(0);
  QCOMPARE(item.data(BuildItemModel::ExternalIdRole).toLongLong(), qint64(5952));
  QCOMPARE(item.data(BuildItemModel::CanonicalIdRole).toString(),
           QString("/Lotus/RevenantPrime"));
  QCOMPARE(item.data(BuildItemModel::NameRole).toString(),
           QString("Revenant Prime"));
}

void BuildSourceModelTest::exposesBuildSummaries() {
  BuildSummaryModel model;
  const QJsonObject response{
      {"builds",
       QJsonArray{QJsonObject{
           {"identity",
            QJsonObject{{"source", "overframe"}, {"external_id", 374539}}},
           {"title", "General use"},
           {"author", QJsonObject{{"username", "Builder"}}},
           {"score", 42},
           {"formas", 3},
           {"updated_at", "2026-08-13T10:00:00Z"},
           {"item", QJsonObject{{"canonical_id", "/Lotus/RevenantPrime"}}},
       }}}};
  QVERIFY(model.replace(response));
  QCOMPARE(model.rowCount(), 1);
  const QModelIndex build = model.index(0);
  QCOMPARE(build.data(BuildSummaryModel::ExternalIdRole).toLongLong(),
           qint64(374539));
  QCOMPARE(build.data(BuildSummaryModel::TitleRole).toString(),
           QString("General use"));
  QCOMPARE(build.data(BuildSummaryModel::AuthorRole).toString(),
           QString("Builder"));
  QCOMPARE(build.data(BuildSummaryModel::FormasRole).toInt(), 3);
}

void BuildSourceModelTest::rejectsMalformedResponses() {
  BuildItemModel items;
  BuildSummaryModel builds;
  QString error;
  QVERIFY(!items.replace(QJsonObject{{"items", QJsonArray{1}}}, &error));
  QVERIFY(!error.isEmpty());
  error.clear();
  QVERIFY(!builds.replace(
      QJsonObject{{"builds", QJsonArray{QJsonObject{{"title", "No id"}}}}},
      &error));
  QVERIFY(!error.isEmpty());
}

QTEST_MAIN(BuildSourceModelTest)

#include "build_source_model_test.moc"
