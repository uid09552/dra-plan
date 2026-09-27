import { Microservice, Scenario } from '../../../core/api/models';

/** Components a scenario affects: the chosen ones, otherwise the default (whole service). */
export function affectedComponents(scenario: Scenario, components: Microservice[]): Microservice[] {
  const ids = scenario.affectedMicroserviceIds ?? [];
  return ids.length
    ? components.filter((c) => ids.includes(c.id))
    : components.filter((c) => c.isDefault);
}
