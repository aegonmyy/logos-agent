#ifndef AGENT_OWNER_PLUGIN_H
#define AGENT_OWNER_PLUGIN_H

#include <QString>
#include <QVariantList>

#include "rep_agent_owner_source.h"
#include "agent_owner_interface.h"
#include "LogosViewPluginBase.h"

class LogosAPI;
class LogosAPIClient;

// AgentOwnerPlugin bridges the QML RemoteObjects layer with the agent core
// module. It owns a LogosAPIClient that calls the agent module's methods.
class AgentOwnerPlugin : public AgentOwnerSimpleSource,
                         public AgentOwnerInterface,
                         public AgentOwnerViewPluginBase
{
    Q_OBJECT
    Q_PLUGIN_METADATA(IID AgentOwnerInterface_iid FILE "metadata.json")
    Q_INTERFACES(AgentOwnerInterface)

public:
    explicit AgentOwnerPlugin(QObject* parent = nullptr);
    ~AgentOwnerPlugin() override;

    QString name()    const override { return QStringLiteral("agent_owner"); }
    QString version() const override { return QStringLiteral("1.0.0"); }

    Q_INVOKABLE void initLogos(LogosAPI* api);

    // Slots declared in the .rep source.
    void refresh() override;
    QString invokeSkill(QString name, QString argsJson) override;

private:
    void ensureClient();
    QString invokeAgent(const QString& method, const QVariantList& args = {});

    LogosAPI*       m_api    = nullptr;
    LogosAPIClient* m_client = nullptr;
};

#endif // AGENT_OWNER_PLUGIN_H
