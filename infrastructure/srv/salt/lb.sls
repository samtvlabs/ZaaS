load_balancer:
  boto_elbv2.load_balancer_present:
    - name: net-lb
    - security_groups: [sg-12345678]
    - subnets: [subnet-12345678]
    - region: us-east-1

web_listener:
  boto_elbv2.listener_present:
    - load_balancer_arn: {{ pillar['load_balancer_arn'] }}
    - protocol: HTTP
    - port: 3000
    - default_actions:
      - type: forward
        target_group_arn: {{ pillar['target_group_arn'] }}
    - region: us-east-1