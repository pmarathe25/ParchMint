#pragma once

#include <QPointer>
#include <QQuickTextDocument>
#include <QSyntaxHighlighter>
#include <qqmlintegration.h>

class MarkdownHighlighter : public QSyntaxHighlighter
{
  Q_OBJECT
  QML_ELEMENT
  Q_PROPERTY(QObject* textDocument READ textDocument WRITE setTextDocument NOTIFY textDocumentChanged)

public:
  explicit MarkdownHighlighter(QObject* parent = nullptr);
  QObject* textDocument() const;
  void setTextDocument(QObject* document);

signals:
  void textDocumentChanged();
  void highlighterError(const QString& message);

protected:
  void highlightBlock(const QString& text) override;

private:
  QPointer<QQuickTextDocument> m_textDocument;
};
