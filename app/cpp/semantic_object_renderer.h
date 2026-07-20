#pragma once

#include <QObject>
#include <QTextObjectInterface>

class SemanticObjectRenderer final : public QObject, public QTextObjectInterface
{
  Q_OBJECT
  Q_INTERFACES(QTextObjectInterface)

public:
  explicit SemanticObjectRenderer(QObject* parent = nullptr);
  QSizeF intrinsicSize(QTextDocument* document,
                       int position,
                       const QTextFormat& format) override;
  void drawObject(QPainter* painter,
                  const QRectF& rect,
                  QTextDocument* document,
                  int position,
                  const QTextFormat& format) override;
};
