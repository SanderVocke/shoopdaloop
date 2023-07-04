#include <QApplication>
#include <QDebug>
#include <QFile>
#include <QLabel>

#include "MaterialDesignIcons.h"

int main(int argc, char *argv[])
{
	QApplication a(argc, argv);

#if 1 // WIDGETS

	auto label = new QLabel;
	label->setPixmap(material::pixmap("eye", QSize(32, 32), Qt::darkGray));

	label->show();

#else // QML

	QQmlApplicationEngine engine;
	engine.load(QUrl(QStringLiteral("main.qml")));
	if (engine.rootObjects().isEmpty())
		return -1;

#endif

	return a.exec();
}
