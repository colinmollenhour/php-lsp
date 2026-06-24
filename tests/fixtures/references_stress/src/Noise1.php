<?php

namespace App;

class Noise1
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
