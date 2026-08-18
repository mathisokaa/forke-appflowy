/**
 * Self-hosted build: upgrade/paywall UI is fully disabled. isAppFlowyHosted()
 * always returns false, and Pro-only display logic below is left intact for
 * users who do have a real subscription plan on record.
 */
import { Subscription, SubscriptionPlan } from '@/application/types';

const PRO_ACCESS_PLANS = new Set([SubscriptionPlan.Pro, SubscriptionPlan.Team]);

export function isAppFlowyHosted(): boolean {
  return false;
}

export function hasProAccessFromPlans(plans?: SubscriptionPlan[] | null): boolean {
  if (!plans || plans.length === 0) return false;
  return plans.some((plan) => PRO_ACCESS_PLANS.has(plan));
}

export function getProAccessPlanFromSubscriptions(subscriptions?: Subscription[] | null): SubscriptionPlan {
  if (!subscriptions || subscriptions.length === 0) return SubscriptionPlan.Free;
  return subscriptions.some((subscription) => PRO_ACCESS_PLANS.has(subscription.plan))
    ? SubscriptionPlan.Pro
    : SubscriptionPlan.Free;
}
