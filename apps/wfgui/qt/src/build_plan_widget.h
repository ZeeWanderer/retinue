#pragma once

#include <QJsonObject>
#include <QWidget>

class QVBoxLayout;

class BuildPlanWidget final : public QWidget {
public:
  explicit BuildPlanWidget(QWidget *parent = nullptr);
  void setPlan(const QJsonObject &baseline, const QJsonObject &result);

private:
  QVBoxLayout *layout_;
  QWidget *body_ = nullptr;
  QJsonObject baseline_;
  QJsonObject result_;
};
