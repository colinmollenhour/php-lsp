<?php

namespace App;

class Target
{
    private function compute(): int
    {
        return 1;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
