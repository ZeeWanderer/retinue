#pragma once

#include <QJsonObject>
#include <QWidget>

class AppController;
class QPainter;

namespace wfgui {

class ArcaneCardWidget final : public QWidget {
public:
  ArcaneCardWidget(AppController *controller, QJsonObject slot,
                   QJsonObject upgrade, QWidget *parent = nullptr);

protected:
  void paintEvent(QPaintEvent *event) override;

private:
  void paintArcane(QPainter &painter) const;
  void paintEmpty(QPainter &painter) const;

  AppController *controller_;
  QJsonObject slot_;
  QJsonObject upgrade_;
  QString id_;
  QString name_;
  QString rarity_;
};

} // namespace wfgui
