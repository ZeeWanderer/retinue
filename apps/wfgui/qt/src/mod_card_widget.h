#pragma once

#include <QColor>
#include <QJsonObject>
#include <QPixmap>
#include <QPointer>
#include <QWidget>

class AppController;
class QEnterEvent;
class QEvent;
class QHideEvent;
class QPainter;
class QVariantAnimation;

namespace wfgui {

QPixmap modPolarityPixmap(const QString &polarity, const QColor &color);

class ModCardPreview;

class ModCardWidget final : public QWidget {
public:
  ModCardWidget(AppController *controller, QJsonObject slot,
                QJsonObject upgrade, QString slotPolarity,
                QWidget *parent = nullptr);
  ~ModCardWidget() override;

protected:
  void paintEvent(QPaintEvent *event) override;
  void enterEvent(QEnterEvent *event) override;
  void leaveEvent(QEvent *event) override;
  void hideEvent(QHideEvent *event) override;
  bool eventFilter(QObject *watched, QEvent *event) override;

private:
  friend class ModCardPreview;

  void paintCard(QPainter &painter, qreal expansion,
                 const QSizeF &canvasSize) const;
  void paintEmpty(QPainter &painter, const QRectF &root, qreal scale) const;
  void drawSlotPolarity(QPainter &painter, const QRectF &root,
                        qreal scale) const;
  void drawRankPips(QPainter &painter, const QRectF &root, qreal scale,
                    qreal width, qreal bottom) const;
  bool showsExternalPolarity() const;
  void openPreview();
  void closePreview(bool animated);
  void animatePreview(qreal target);

  AppController *controller_;
  QJsonObject slot_;
  QJsonObject upgrade_;
  QString role_;
  QString slotPolarity_;
  QString modPolarity_;
  QString polarityState_;
  QString id_;
  QPointer<ModCardPreview> preview_;
  QVariantAnimation *previewAnimation_;
};

} // namespace wfgui
