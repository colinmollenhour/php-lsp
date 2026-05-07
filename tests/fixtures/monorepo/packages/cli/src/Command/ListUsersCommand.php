<?php
namespace Acme\Cli\Command;

use Acme\Core\Repository\UserRepository;

class ListUsersCommand {
    public function __construct(
        private UserRepository $repository,
    ) {}

    public function execute(): void {
        $users = $this->repository->findAll();
        foreach ($users as $user) {
            echo $user->getDisplayName() . PHP_EOL;
        }
    }
}
