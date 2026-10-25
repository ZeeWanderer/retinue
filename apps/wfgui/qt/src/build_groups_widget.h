#pragma once

#include <QJsonObject>
#include <QWidget>

class AppController;
class BuildPlanWidget;
class BuildTopologyWidget;
class QCheckBox;
class QComboBox;
class QLabel;
class QPushButton;
class QScrollArea;
class QStackedWidget;

class BuildGroupsWidget final : public QWidget {
  Q_OBJECT

public:
  explicit BuildGroupsWidget(AppController *controller, QWidget *parent = nullptr);
  void selectGroup(const QString &id);

signals:
  void equipmentRequested(const QString &definitionId, const QString &instanceId);
  void discoverRequested(const QString &definitionId);

private:
  bool eventFilter(QObject *watched, QEvent *event) override;
  void refresh();
  void refreshTargets();
  void showMember();
  void saveOptions();
  void calculate();

  AppController *controller_;
  QStackedWidget *pages_;
  QStackedWidget *contentPages_;
  QWidget *editor_;
  QWidget *emptyPage_;
  QScrollArea *scroll_;
  QWidget *actions_;
  QComboBox *groups_;
  QComboBox *target_;
  QComboBox *members_;
  QLabel *meta_;
  QLabel *state_;
  QLabel *emptyTitle_;
  QLabel *emptyDescription_;
  QPushButton *emptyEquipment_;
  QPushButton *emptyDiscover_;
  QCheckBox *preserveSlots_;
  QCheckBox *allowOmni_;
  QCheckBox *allowUmbral_;
  BuildPlanWidget *plan_;
  BuildTopologyWidget *topology_;
  QLabel *capacity_;
  QLabel *notes_;
  QPushButton *original_;
  QPushButton *planned_;
  QPushButton *remove_;
  QPushButton *calculate_;
  QPushButton *equipment_;
  QPushButton *discover_;
  QString selectedId_;
  QJsonObject group_;
  QJsonObject rendered_;
  bool refreshing_ = false;
  bool planning_ = false;
};
