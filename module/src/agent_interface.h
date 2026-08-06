#ifndef AGENT_INTERFACE_H
#define AGENT_INTERFACE_H

#include <QObject>
#include <QString>

#include "interface.h"
#include "logos_types.h"

// Qt plugin interface for the autonomous agent module. All methods are
// JSON-out — callers parse the returned QString as a JSON object containing at
// minimum {"ok": true|false}. The agent logic lives in the Rust core
// (liblogos_agent.so), which this module loads and calls.
class AgentInterface : public PluginInterface
{
public:
    virtual ~AgentInterface() = default;

    // Liveness: reports whether the Rust agent core is loadable.
    Q_INVOKABLE virtual QString health() = 0;

    // Version of the Rust agent core.
    Q_INVOKABLE virtual QString agentVersionJson() = 0;

    // Catalogue of the agent's default skills (Storage, Messaging, Blockchain,
    // Meta), as a JSON document the app can render.
    Q_INVOKABLE virtual QString skillsJson() = 0;

    // Start a live agent session bound to a shielded account id.
    Q_INVOKABLE virtual QString startSessionJson(const QString& accountId) = 0;

    // Invoke a skill by name with JSON arguments; returns the JSON result.
    Q_INVOKABLE virtual QString invokeSkillJson(const QString& name,
                                                const QString& argsJson) = 0;
};

#define AgentInterface_iid "org.logos.AgentInterface"
Q_DECLARE_INTERFACE(AgentInterface, AgentInterface_iid)

#endif // AGENT_INTERFACE_H
