#pragma once

#include <QJsonObject>
#include <QWidget>

class AppController;
class BuildEquipmentFilterModel;
class BuildTopologyWidget;
class QComboBox;
class QFrame;
class QLabel;
class QLineEdit;
class QListView;
class QModelIndex;
class QPushButton;
class QResizeEvent;
class QSplitter;
class QToolButton;
class QVariantAnimation;

class BuildEquipmentWidget final : public QWidget {
  Q_OBJECT

public:
  explicit BuildEquipmentWidget(AppController *controller,
                                QWidget *parent = nullptr);
  void selectDefinition(const QString &definitionId,
                        const QString &instanceId = QString());

signals:
  void groupRequested(const QString &groupId);

protected:
  void resizeEvent(QResizeEvent *event) override;

private:
  void restoreSelection();
  void selectEquipment(const QModelIndex &index);
  void selectCopy(int index);
  void selectConfig(int index);
  void updateState();
  void updateResponsiveLayout();
  void updateRailWidth();
  int preferredRailWidth() const;
  void setRailCollapsed(bool collapsed, bool animated);
  void updateCollapseControl();
  void showGroupMenu();
  void createGroup(bool captureConfig);
  void addConfig(const QJsonObject &group);

  AppController *controller_;
  BuildEquipmentFilterModel *equipment_;
  QSplitter *splitter_;
  QFrame *rail_;
  QFrame *detail_;
  QLineEdit *search_;
  QComboBox *category_;
  QListView *list_;
  QLabel *empty_;
  QToolButton *collapse_;
  QVariantAnimation *railAnimation_;
  QPushButton *back_;
  QLabel *title_;
  QLabel *meta_;
  QComboBox *copy_;
  QComboBox *config_;
  BuildTopologyWidget *topology_;
  QPushButton *createGroup_;
  QPushButton *captureConfig_;
  QPushButton *openGroup_;
  QLabel *state_;
  QJsonObject instance_;
  QString selectedDefinitionId_;
  QString selectedInstanceId_;
  QString pendingGroupId_;
  QString addedGroupId_;
  bool pendingCreate_ = false;
  bool pendingCapture_ = false;
  bool railCollapsed_ = false;
  bool narrowDetail_ = false;
};
