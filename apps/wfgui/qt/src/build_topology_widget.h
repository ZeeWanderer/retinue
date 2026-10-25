#pragma once

#include <QJsonObject>
#include <QWidget>

class QVBoxLayout;
class AppController;

class BuildTopologyWidget final : public QWidget {
  Q_OBJECT

public:
  explicit BuildTopologyWidget(AppController *controller = nullptr,
                               QWidget *parent = nullptr);

  void setPlayerInstance(const QJsonObject &instance, int configIndex);
  void setPlayerSnapshot(const QJsonObject &snapshot);
  void setSourceRevision(const QJsonObject &revision,
                         const QJsonObject &baseline);
  void setPlanResult(const QJsonObject &result, const QJsonObject &baseline);
  void clear();

private:
  void render(const QJsonObject &topology, const QJsonArray &upgrades,
              const QJsonArray &polarities, const QJsonArray &shards,
              const QJsonArray &abilityOverrides);
  void resetBody();

  QVBoxLayout *layout_;
  QWidget *body_ = nullptr;
  AppController *controller_;
};
