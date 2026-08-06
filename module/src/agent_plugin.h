#ifndef AGENT_PLUGIN_H
#define AGENT_PLUGIN_H

#include <QObject>
#include <QString>
#include <QVariantList>

#include "agent_ffi_client.h"
#include "agent_interface.h"
#include "logos_api.h"

class AgentPlugin : public QObject, public AgentInterface
{
    Q_OBJECT
    Q_PLUGIN_METADATA(IID AgentInterface_iid FILE "metadata.json")
    Q_INTERFACES(AgentInterface PluginInterface)

public:
    AgentPlugin();
    ~AgentPlugin() override;

    QString name()    const override { return QStringLiteral("agent"); }
    QString version() const override { return QStringLiteral("1.0.0"); }

    Q_INVOKABLE void initLogos(LogosAPI* api);

    Q_INVOKABLE QString health()          override;
    Q_INVOKABLE QString agentVersionJson() override;
    Q_INVOKABLE QString skillsJson()      override;
    Q_INVOKABLE QString startSessionJson(const QString& accountId) override;
    Q_INVOKABLE QString invokeSkillJson(const QString& name,
                                        const QString& argsJson)   override;

signals:
    void eventResponse(const QString& name, const QVariantList& args);

private:
    AgentFfiClient m_ffi;
    LogosAPI*      m_api = nullptr;
};

#endif // AGENT_PLUGIN_H
